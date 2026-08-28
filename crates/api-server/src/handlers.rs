//! Chakra REST handlers (T4.3).
//!
//! Envelope `{success, data, error:{code,message}}`; integer
//! `price_impact_bps`; quotes never hit RPC (Redis / memory pool store only);
//! `/ready` = snapshot current AND ≥1 pool key.

use {
    crate::{
        catalog,
        envelope::{ApiError, ApiErrorCode, Envelope},
        evm_balances, hydrate,
        state::AppState,
    },
    axum::{
        extract::{Query, State},
        http::StatusCode,
        response::IntoResponse,
        Json,
    },
    market_snapshot::decimals,
    router_engine::{types::RouteRequest, TokenId},
    serde::{Deserialize, Serialize},
    serde_json::{json, Value},
};

// ─── Landing ────────────────────────────────────────────────────────────────

pub async fn api_root() -> impl IntoResponse {
    Json(json!({
        "service": "Chakra API",
        "status": "ok",
        "endpoints": {
            "health": "/api/v1/health",
            "ready": "/api/v1/ready",
            "quote": "/api/v1/quote",
            "build_tx": "/api/v1/build_tx",
            "tokens": "/api/v1/tokens",
            "balances": "/api/v1/balances"
        },
        "docs": { "openapi": "docs/openapi.yaml" }
    }))
}

fn err_response(code: ApiErrorCode, message: impl Into<String>) -> (StatusCode, Json<Envelope<Value>>) {
    (StatusCode::BAD_REQUEST, Json(Envelope::err(code, message)))
}

fn err_response_code(status: StatusCode, error: ApiError) -> (StatusCode, Json<Envelope<Value>>) {
    (status, Json(Envelope::<Value>::err_from(error)))
}

// ─── GET /api/v1/quote ──────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct QuoteQuery {
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub amount_in: Option<String>,
    /// Slippage in bps (integer; default 50).
    pub slippage_bps: Option<u32>,
    /// Path-finder hop limit. Omit = server default.
    pub max_hops: Option<usize>,
    /// Max parallel sub-routes in a split quote (clamped to server max).
    pub max_splits: Option<usize>,
}

#[derive(Serialize)]
pub struct QuoteData {
    pub amount_in: String,
    pub expected_output: String,
    pub minimum_output: String,
    /// Integer basis points (never a float).
    pub price_impact_bps: u32,
    pub protocol_fee_bps: u32,
    pub is_split: bool,
    pub max_splits: usize,
    pub sub_routes: Vec<SubRouteData>,
    pub compute_time_ms: u64,
}

#[derive(Serialize)]
pub struct SubRouteData {
    pub source: String,
    pub path: Vec<String>,
    pub pool_addresses: Vec<String>,
    /// Per-hop DEX type (`xyk` | `stable` | `clmm` | …) — T4.7 explicit hop
    /// metadata. Length == `pool_addresses`. UI/SDK must not infer the DEX
    /// type from the joined `source` string.
    pub dex_types: Vec<String>,
    /// Per-hop venue fee in bps (T4.7). Same length as `pool_addresses`.
    pub hop_fees: Vec<u32>,
    /// Per-hop allowlisted factory (empty string = legacy pool). T4.7.
    pub hop_factories: Vec<String>,
    pub amount_in: String,
    pub amount_out: String,
    pub fraction_bps: u32,
}

fn parse_token(token: &str, mbtc: &str) -> Result<String, ApiError> {
    let token = token.trim().to_ascii_lowercase();
    if token.is_empty() {
        return Err(ApiError::new(ApiErrorCode::InvalidParams, "token must not be empty"));
    }
    if decimals::is_native_usdc_encoding(&token) {
        return Err(ApiError::new(
            ApiErrorCode::UnknownToken,
            "native USDC is gas only — never a swap token (SC-12)",
        ));
    }
    if !catalog::is_catalog_swap_token_with(&token, mbtc) {
        return Err(ApiError::new(
            ApiErrorCode::UnknownToken,
            format!("unknown token: {token}"),
        ));
    }
    Ok(token)
}

fn parse_amount_in(raw: &str) -> Result<u128, ApiError> {
    let amount: u128 = raw
        .trim()
        .parse()
        .map_err(|_| ApiError::new(ApiErrorCode::InvalidParams, "amount_in must be an unsigned integer"))?;
    if amount == 0 {
        return Err(ApiError::new(ApiErrorCode::ZeroAmount, "amount_in must be positive"));
    }
    Ok(amount)
}

pub async fn get_quote(State(state): State<AppState>, Query(params): Query<QuoteQuery>) -> impl IntoResponse {
    let mbtc = state.mbtc_address.clone();
    let token_in = match parse_token(params.token_in.as_deref().unwrap_or_default(), &mbtc) {
        Ok(t) => t,
        Err(error) => return err_response_code(StatusCode::BAD_REQUEST, error),
    };
    let token_out = match parse_token(params.token_out.as_deref().unwrap_or_default(), &mbtc) {
        Ok(t) => t,
        Err(error) => return err_response_code(StatusCode::BAD_REQUEST, error),
    };
    if token_in == token_out {
        return err_response(ApiErrorCode::SameToken, "token_in and token_out must differ");
    }
    let amount_in = match parse_amount_in(params.amount_in.as_deref().unwrap_or_default()) {
        Ok(a) => a,
        Err(error) => return err_response_code(StatusCode::BAD_REQUEST, error),
    };
    let slippage_bps = params.slippage_bps.unwrap_or(50);
    let max_splits = params
        .max_splits
        .map(|v| v.min(state.config.max_splits))
        .unwrap_or(state.config.max_splits);

    // ── Per-request version check ──
    // Read the current snapshot version pointer. If it matches the loaded
    // engine, use it immediately. Otherwise, reload on change.
    let engine = if let Some(store) = &state.snapshot_store {
        match store.load_current_version().await {
            Ok(pointer_version) => match state.engine_for_version(&pointer_version).await {
                Ok(engine) => engine,
                Err(crate::state::EngineError::NoStore) => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(Envelope::<Value>::err(
                            ApiErrorCode::NotReady,
                            "no snapshot store configured",
                        )),
                    );
                }
                Err(_) => {
                    // Reload failed — try to fall back to a stale engine.
                    match state.best_effort_engine().await {
                        Some(engine) => engine,
                        None => {
                            return (
                                StatusCode::SERVICE_UNAVAILABLE,
                                Json(Envelope::<Value>::err(ApiErrorCode::NotReady, "engine not ready")),
                            );
                        }
                    }
                }
            },
            Err(_) => {
                // Pointer read failed — try stale engine.
                match state.best_effort_engine().await {
                    Some(engine) => engine,
                    None => {
                        return (
                            StatusCode::SERVICE_UNAVAILABLE,
                            Json(Envelope::<Value>::err(ApiErrorCode::NotReady, "engine not ready")),
                        );
                    }
                }
            }
        }
    } else {
        // No snapshot store — use current engine (embedded mode or cold start).
        match state.best_effort_engine().await {
            Some(engine) => engine,
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(Envelope::<Value>::err(ApiErrorCode::NotReady, "engine not ready")),
                );
            }
        }
    };

    let request = RouteRequest {
        token_in: TokenId::Contract { address: token_in },
        token_out: TokenId::Contract { address: token_out },
        amount_in,
        slippage_bps: Some(slippage_bps),
        max_hops: params.max_hops,
        max_splits: Some(max_splits),
        prefer_arc: None,
    };

    let hydration = hydrate::hydrate_for_quote(&state, &engine, &request).await;
    let paths = engine.find_candidate_paths(&request).await;
    let route = engine.get_route_with_paths(&request, &paths, Some(&hydration)).await;

    if route.sub_orders.is_empty() {
        return (
            StatusCode::OK,
            Json(Envelope::<Value>::err(
                ApiErrorCode::NoRoute,
                "No route available for this pair",
            )),
        );
    }

    let sub_routes = route
        .sub_orders
        .iter()
        .map(|so| SubRouteData {
            source: so.path.sources.join(" → "),
            path: so.path.tokens.iter().map(|t| t.canonical()).collect(),
            pool_addresses: so.path.pool_addresses.clone(),
            dex_types: so.path.dex_types.clone(),
            hop_fees: so.path.fee_bps.clone(),
            hop_factories: so.path.factories.clone(),
            amount_in: so.amount_in.to_string(),
            amount_out: so.expected_amount_out.to_string(),
            fraction_bps: (so.fraction * 10_000.0).round() as u32,
        })
        .collect();

    (
        StatusCode::OK,
        Json(Envelope::ok(json!(QuoteData {
            amount_in: route.total_amount_in.to_string(),
            expected_output: route.total_expected_out.to_string(),
            minimum_output: route.minimum_out.to_string(),
            price_impact_bps: route.price_impact_bps,
            protocol_fee_bps: route.protocol_fee_bps,
            is_split: route.is_split,
            max_splits,
            sub_routes,
            compute_time_ms: route.compute_time_ms,
        }))),
    )
}

// ─── GET /api/v1/tokens ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TokenInfo {
    pub symbol: String,
    pub address: String,
    pub decimals: u8,
}

#[derive(Serialize)]
pub struct TokensData {
    pub tokens: Vec<TokenInfo>,
}

pub async fn list_tokens(State(state): State<AppState>) -> impl IntoResponse {
    let tokens = catalog::catalog_swap_tokens_with(&state.mbtc_address)
        .into_iter()
        .map(|t| TokenInfo {
            symbol: t.symbol.to_string(),
            address: t.address.to_ascii_lowercase(),
            decimals: t.decimals,
        })
        .collect();
    Json(Envelope::ok(TokensData { tokens }))
}

// ─── GET /api/v1/balances ───────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BalancesQuery {
    pub account: String,
}

pub async fn get_balances(State(state): State<AppState>, Query(query): Query<BalancesQuery>) -> impl IntoResponse {
    let account = query.account.trim().to_ascii_lowercase();
    if !account.starts_with("0x") || account.len() != 42 {
        return err_response(ApiErrorCode::InvalidParams, "account must be a 0x-prefixed EVM address");
    }
    let Some(rpc) = &state.evm_rpc else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Envelope::<Value>::err(ApiErrorCode::NotReady, "EVM RPC not configured")),
        );
    };
    match evm_balances::fetch_balances(rpc, &account, &state.mbtc_address).await {
        Ok(balances) => (StatusCode::OK, Json(Envelope::ok(serde_json::Value::Object(balances)))),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(Envelope::<Value>::err(
                ApiErrorCode::RpcError,
                format!("balances fetch failed: {error}"),
            )),
        ),
    }
}

// ─── GET /api/v1/health & /api/v1/ready ─────────────────────────────────────

#[derive(Serialize)]
pub struct HealthData {
    pub status: &'static str,
}

pub async fn health_check() -> impl IntoResponse {
    Json(Envelope::ok(HealthData { status: "ok" }))
}

#[derive(Serialize)]
pub struct ReadyData {
    pub status: String,
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_keys: Option<Vec<String>>,
}

pub async fn readiness_check(State(state): State<AppState>) -> impl IntoResponse {
    let ready = state.ready().await;
    let loaded_version = state.loaded_version().await;
    let (status_code, data) = match ready {
        Some((snapshot_id, pool_keys)) => (
            StatusCode::OK,
            ReadyData {
                status: "ready".to_string(),
                ready: true,
                snapshot_id: if snapshot_id.is_empty() {
                    loaded_version
                } else {
                    Some(snapshot_id)
                },
                pool_keys: Some(pool_keys),
            },
        ),
        None => {
            // Report loaded engine version even when not fully ready
            // (stale but usable engine).
            if loaded_version.is_some() {
                // Has a stale engine — still warming but with data.
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    ReadyData {
                        status: "warming_up".to_string(),
                        ready: false,
                        snapshot_id: loaded_version,
                        pool_keys: None,
                    },
                )
            } else {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    ReadyData {
                        status: "warming_up".to_string(),
                        ready: false,
                        snapshot_id: None,
                        pool_keys: None,
                    },
                )
            }
        }
    };
    (status_code, Json(Envelope::ok(data)))
}

// ─── POST /api/v1/build_tx (stub until T4.4) ────────────────────────────────

#[derive(Deserialize)]
pub struct BuildTxRequest {
    pub user: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub min_amount_out: String,
    pub sub_routes: Vec<BuildTxSubRoute>,
}

#[derive(Deserialize)]
pub struct BuildTxSubRoute {
    pub amount_in: String,
    pub steps: Vec<BuildTxStep>,
}

#[derive(Deserialize)]
pub struct BuildTxStep {
    pub dex_type: String,
    pub pool_address: String,
    pub token_in: String,
    pub token_out: String,
    /// Per-hop fee in bps from the quote snapshot. The server validates this
    /// against the snapshot; omitting it falls back to the venue default.
    #[serde(default)]
    pub fee_bps: Option<u32>,
}

#[derive(Serialize)]
pub struct BuildTxDataEnvelope {
    pub to: String,
    pub data: String,
    pub chain_id: u64,
    pub value: String,
    pub deadline: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typed_data: Option<Value>,
    pub required_approvals: Vec<Value>,
}

pub async fn build_tx(State(state): State<AppState>, Json(body): Json<BuildTxRequest>) -> impl IntoResponse {
    // T4.4: splitSwap calldata encoder (not a re-quoter).
    if body.user.trim().is_empty() || !body.user.starts_with("0x") {
        return err_response(ApiErrorCode::InvalidParams, "user must be a 0x-prefixed EVM address");
    }
    for token in [&body.token_in, &body.token_out] {
        if let Err(error) = parse_token(token, &state.mbtc_address) {
            return err_response_code(StatusCode::BAD_REQUEST, error);
        }
    }
    match crate::build_tx::build_tx_data(&state, &body).await {
        Ok((data, deadline, typed_data, required_approvals)) => (
            StatusCode::OK,
            Json(Envelope::ok(json!(BuildTxDataEnvelope {
                to: state.config.chakra_aggregator.clone(),
                data,
                chain_id: 5042002,
                value: "0".to_string(),
                deadline,
                typed_data,
                required_approvals,
            }))),
        ),
        Err(error) => {
            let message = error.to_string();
            let code = if message == ApiErrorCode::NotReady.as_str() {
                ApiErrorCode::NotReady
            } else if message == ApiErrorCode::Paused.as_str() {
                ApiErrorCode::Paused
            } else {
                ApiErrorCode::RouteInvalid
            };
            let status = match code {
                ApiErrorCode::NotReady | ApiErrorCode::Paused => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::BAD_REQUEST,
            };
            (status, Json(Envelope::<Value>::err(code, message)))
        }
    }
}
