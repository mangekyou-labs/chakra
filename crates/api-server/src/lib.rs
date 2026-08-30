pub mod abi;
pub mod build_tx;
pub mod catalog;
pub mod config;
pub mod envelope;
pub mod evm_balances;
pub mod handlers;
pub mod hydrate;
pub mod rate_limit;
pub mod snapshot_loader;
pub mod state;

use {
    axum::{
        http::header::HeaderValue,
        middleware,
        routing::{get, post},
        Router,
    },
    config::AppConfig,
    rate_limit::RateLimitState,
    state::AppState,
    std::{net::SocketAddr, path::PathBuf},
    tower_http::{
        cors::{AllowOrigin, CorsLayer},
        limit::RequestBodyLimitLayer,
        services::ServeDir,
    },
    tracing::info,
};

/// Build the Chakra API router. Public so integration tests can drive it via
/// `tower::ServiceExt` with injected `ConnectInfo`.
pub fn build_router(app_state: AppState, rate_limit: RateLimitState) -> Router {
    let cors_origins = app_state.config.chakra_cors_origins.clone();

    let api = Router::new()
        .route("/", get(handlers::api_root))
        .route("/api/v1/quote", get(handlers::get_quote))
        .route("/api/v1/build_tx", post(handlers::build_tx))
        .route("/api/v1/tokens", get(handlers::list_tokens))
        .route("/api/v1/balances", get(handlers::get_balances))
        .route("/api/v1/health", get(handlers::health_check))
        .route("/api/v1/ready", get(handlers::readiness_check))
        .layer(middleware::from_fn_with_state(
            rate_limit,
            rate_limit::rate_limit_middleware,
        ))
        .with_state(app_state);

    let origins: Vec<String> = if cors_origins.is_empty() {
        vec!["http://localhost:3000".to_string()]
    } else {
        cors_origins
    };
    let parsed_origins: Vec<HeaderValue> = origins
        .iter()
        .map(|o| HeaderValue::from_str(o).expect("valid CORS origin"))
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed_origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let app = Router::new()
        .merge(api)
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(128 * 1024));

    let logo_dir = PathBuf::from(std::env::var("TOKEN_LOGO_DIR").unwrap_or_else(|_| "data/logos".into()));
    let _ = std::fs::create_dir_all(&logo_dir);
    app.nest_service("/logos", ServeDir::new(logo_dir))
}

pub async fn run_server() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    let listen_addr: SocketAddr = config.listen_addr.parse()?;
    let state = AppState::from_env().await?;
    let rate_limit = RateLimitState::from_env();
    let app = build_router(state.clone(), rate_limit);

    info!(
        "Chakra API listening on {} (rpc={})",
        listen_addr, config.chakra_rpc_http
    );

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        axum::http::{header::CONTENT_TYPE, Request, StatusCode},
        tower::ServiceExt,
    };

    #[tokio::test]
    async fn serves_logo_files_from_configured_directory() {
        let dir = std::env::temp_dir().join(format!(
            "chakra-logo-serve-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sample = dir.join("sample.svg");
        std::fs::write(&sample, b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap();

        let app = axum::Router::new().nest_service("/logos", tower_http::services::ServeDir::new(&dir));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/logos/sample.svg")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(content_type.starts_with("image/svg+xml"), "got {content_type}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
