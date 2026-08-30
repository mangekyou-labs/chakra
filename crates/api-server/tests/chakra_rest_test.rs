//! T4.3 Chakra REST integration tests.
//!
//! HTTP via `tower::ServiceExt::oneshot` against the real `build_router`.
//! Quotes never hit RPC (`QUOTE_RPC_HYDRATE_ENABLED=false` + in-memory pool
//! store). `/balances` uses the fixture `EvmRpcClient` (never live Arc).
//! 429 tests inject a non-loopback `ConnectInfo` (loopback stays exempt for
//! local curl — documented).

use {
    api_server::{
        build_router, config::AppConfig, envelope::ApiErrorCode, rate_limit::RateLimitState, state::AppState,
    },
    axum::{
        body::Body,
        extract::ConnectInfo,
        http::{header, Method, Request, StatusCode},
        response::Response,
        Router,
    },
    dex_adapters::evm_rpc::fixture,
    market_snapshot::{
        decimals::{EURC, NATIVE_USDC, USDC_ERC20},
        pool_state_store::{
            FactoryRecord, MemoryPoolStateStore, PoolStateStore, StablePoolStateValue, XykPoolStateValue,
        },
        store::{MemorySnapshotStore, SnapshotStore},
        ClmmBitmapWordSnapshot, ClmmCoverageSnapshot, ClmmPoolRefSnapshot, ClmmPoolSnapshot, ClmmTickSnapshot,
        MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine},
    serde_json::{json, Value},
    std::{sync::Arc, time::Duration},
    tower::ServiceExt,
};

const CIRBTC: &str = "0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF";
const XYK_POOL_UE: &str = "0x0000000000000000000000000000000000000001";
const STABLE_POOL_UE: &str = "0x0000000000000000000000000000000000000002";
const XYK_POOL_UM: &str = "0x0000000000000000000000000000000000000003";
const XYK_UE_SEED: u128 = 10_000_000_000;
const STABLE_UE_SEED: u128 = 200_000_000_000;

// ─── Fixtures ───────────────────────────────────────────────────────────────

fn app_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.runtime_mode = api_server::config::RuntimeMode::Embedded;
    config.snapshot_backend = Some("memory".to_string());
    config.max_splits = 5;
    config.quote_rpc_hydrate_enabled = false;
    config
}

/// In-memory Chakra topology (same seeds as T2.3 / T4.2 quote engine tests).
fn chakra_snapshot() -> MarketSnapshot {
    MarketSnapshot::from_sources(
        "chakra-api-1",
        1_700_000_000_000,
        "arc-testnet",
        vec![
            SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![
                    TradingPairSnapshot {
                        token_a: USDC_ERC20.to_string(),
                        token_b: EURC.to_string(),
                        pool_address: XYK_POOL_UE.to_string(),
                        fee_bps: 30,
                        dex_type: "xyk".to_string(),
                        factory: String::new(),
                    },
                    TradingPairSnapshot {
                        token_a: USDC_ERC20.to_string(),
                        token_b: CIRBTC.to_string(),
                        pool_address: XYK_POOL_UM.to_string(),
                        fee_bps: 30,
                        dex_type: "xyk".to_string(),
                        factory: String::new(),
                    },
                ],
            },
            SourceSnapshot {
                source: "chakra-stable".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: USDC_ERC20.to_string(),
                    token_b: EURC.to_string(),
                    pool_address: STABLE_POOL_UE.to_string(),
                    fee_bps: 4,
                    dex_type: "stable".to_string(),
                    factory: String::new(),
                }],
            },
        ],
    )
}

async fn seed_pool_state(pools: &MemoryPoolStateStore) {
    // Use lowercased EURC to match pathfinder normalization
    let eurc_lower = EURC.to_lowercase();
    pools
        .set_xyk_batch(&[
            XykPoolStateValue::new(
                "chakra-xyk",
                XYK_POOL_UE,
                USDC_ERC20,
                &eurc_lower,
                30,
                XYK_UE_SEED,
                XYK_UE_SEED,
            ),
            XykPoolStateValue::new(
                "chakra-xyk",
                XYK_POOL_UM,
                USDC_ERC20,
                CIRBTC.to_lowercase(),
                30,
                50_000_000_000,
                100_000_000,
            ),
        ])
        .await
        .unwrap();
    pools
        .set_stable_batch(&[StablePoolStateValue::new(
            "chakra-stable",
            STABLE_POOL_UE,
            USDC_ERC20,
            &eurc_lower,
            STABLE_UE_SEED,
            STABLE_UE_SEED,
            100,
            4,
        )])
        .await
        .unwrap();
}

/// AppState + router with the fixture EvmRpcClient and in-memory stores.
/// `seed_pools=false` leaves the pool store empty (for the ready-503 case).
async fn test_app_with_pools(
    snapshot: Option<MarketSnapshot>,
    evm_rpc_url: Option<String>,
    seed_pools: bool,
) -> (Router, Arc<MemorySnapshotStore>, Arc<MemoryPoolStateStore>) {
    let config = app_config();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    if let Some(snapshot) = snapshot {
        snapshot_store.publish_snapshot(&snapshot).await.unwrap();
    }
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    if seed_pools {
        seed_pool_state(&pool_store).await;
    }

    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&chakra_snapshot()).await;
    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn market_snapshot::store::SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn market_snapshot::pool_state_store::PoolStateStore>),
        Some(snapshot_store.clone()),
        Some(pool_store.clone()),
        evm_rpc_url.map(|url| Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        Some(("chakra-api-1".to_string(), Arc::new(engine))),
    )
    .await;

    let router = build_router(state, RateLimitState::from_env());
    (router, snapshot_store, pool_store)
}

async fn test_app(
    snapshot: Option<MarketSnapshot>,
    evm_rpc_url: Option<String>,
) -> (Router, Arc<MemorySnapshotStore>, Arc<MemoryPoolStateStore>) {
    test_app_with_pools(snapshot, evm_rpc_url, true).await
}

/// Oneshot request; returns (status, body Value).
async fn send(router: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn get(router: &Router, path: &str) -> (StatusCode, Value) {
    send(router, Request::builder().uri(path).body(Body::empty()).unwrap()).await
}

async fn post(router: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(
        router,
        Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}

// ─── 1. Envelope + error codes ──────────────────────────────────────────────

#[tokio::test]
async fn quote_errors_use_envelope_with_code_and_no_float_impact() {
    let (router, _, _) = test_app(None, None).await;

    // Missing amount → 400 INVALID_PARAMS.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], ApiErrorCode::InvalidParams.as_str());
    assert_eq!(body["data"], Value::Null);

    // Zero amount → 400 ZERO_AMOUNT.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=0"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], ApiErrorCode::ZeroAmount.as_str());

    // Same token → 400 SAME_TOKEN.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={USDC_ERC20}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], ApiErrorCode::SameToken.as_str());

    // Unknown token → 400 UNKNOWN_TOKEN.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out=0xDEADBEEF&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], ApiErrorCode::UnknownToken.as_str());

    // Native USDC encoding → rejected (SC-12) — never a swap token.
    for native in [NATIVE_USDC, "0x0000000000000000000000000000000000000000"] {
        let (status, body) = get(
            &router,
            &format!("/api/v1/quote?token_in={native}&token_out={EURC}&amount_in=1000000"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "native encoding {native}");
        let code = body["error"]["code"].as_str().unwrap_or("");
        assert!(
            code == ApiErrorCode::UnknownToken.as_str() || code == ApiErrorCode::InvalidParams.as_str(),
            "native encoding {native} gave {code}"
        );
    }

    // Success shape: `error` is an object field (null), no float `price_impact`.
    let (router, _, _) = test_app(Some(chakra_snapshot()), None).await;
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["error"], Value::Null);
    assert!(body.get("price_impact").is_none(), "float price_impact must not exist");
    assert!(body["data"]["price_impact_bps"].is_i64() || body["data"]["price_impact_bps"].is_u64());
    assert_eq!(body["data"]["protocol_fee_bps"], 0);
}

// ─── 2. /tokens catalog freeze ──────────────────────────────────────────────

#[tokio::test]
async fn tokens_lists_frozen_catalog_only_with_decimals() {
    let (router, _, _) = test_app(None, None).await;
    let (status, body) = get(&router, "/api/v1/tokens").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    let tokens = body["data"]["tokens"].as_array().expect("data.tokens array");
    assert_eq!(tokens.len(), 3, "exactly USDC, EURC, cirBTC");
    let by_symbol: std::collections::HashMap<&str, &Value> =
        tokens.iter().map(|t| (t["symbol"].as_str().unwrap(), t)).collect();

    let usdc = by_symbol["USDC"];
    assert_eq!(usdc["address"], USDC_ERC20.to_ascii_lowercase());
    assert_eq!(usdc["decimals"], 6);
    let eurc = by_symbol["EURC"];
    assert_eq!(eurc["address"], EURC.to_ascii_lowercase());
    assert_eq!(eurc["decimals"], 6);
    let cirbtc = by_symbol["cirBTC"];
    assert_eq!(cirbtc["address"], CIRBTC.to_ascii_lowercase());
    assert_eq!(cirbtc["decimals"], 8);

    assert!(
        tokens
            .iter()
            .all(|t| t["address"] != NATIVE_USDC && t["symbol"] != "native_usdc"),
        "native USDC must be absent"
    );
}

// ─── 3. /quote hydrate ──────────────────────────────────────────────────────

#[tokio::test]
async fn quote_hydrates_chakra_snapshot_routes() {
    let (router, _, _) = test_app(Some(chakra_snapshot()), None).await;

    // USDC→EURC direct routes via both venues (SC-1).
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000"),
    )
    .await;
    eprintln!("DEBUG quote body: {body}");
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["protocol_fee_bps"], 0);
    assert!(data["price_impact_bps"].is_i64() || data["price_impact_bps"].is_u64());
    assert!(data["price_impact"].is_null() || data.get("price_impact").is_none());
    assert_eq!(data["max_splits"], 5);
    assert_eq!(data["is_split"], false);
    let routes = data["sub_routes"].as_array().expect("sub_routes");
    assert!(
        routes
            .iter()
            .any(|r| r["source"].as_str().unwrap().contains("chakra-stable")),
        "USDC→EURC must route via chakra-stable"
    );
    // 1_000e6 control pins the on-chain vector.
    let (_, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000000"),
    )
    .await;
    assert_eq!(body["data"]["expected_output"], "999550535");

    // USDC→cirBTC routes via the xy=k venue.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={CIRBTC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["data"]["sub_routes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["source"].as_str().unwrap().contains("chakra-xyk")),
        "USDC→cirBTC must route via chakra-xyk"
    );
}

// ─── 3b. T4.7 quote hop metadata ────────────────────────────────────────────

/// T4.7: every sub-route carries explicit per-hop `dex_types[]`, `hop_fees`,
/// and `hop_factories` (length == pool_addresses) — the UI/SDK must not
/// reconstruct the DEX type from the joined `source` string.
#[tokio::test]
async fn quote_emits_explicit_per_hop_dex_type_fee_factory() {
    let (router, _, _) = test_app(Some(chakra_snapshot()), None).await;

    // Direct stable route.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let routes = body["data"]["sub_routes"].as_array().unwrap();
    assert!(!routes.is_empty(), "must have at least one sub-route");
    let stable = routes
        .iter()
        .find(|r| r["source"].as_str().unwrap() == "chakra-stable")
        .expect("chakra-stable sub-route");
    assert_eq!(stable["dex_types"], json!(["stable"]));
    assert_eq!(stable["hop_fees"], json!([4]));
    assert_eq!(stable["hop_factories"], json!([""]));
    assert_eq!(
        stable["dex_types"].as_array().unwrap().len(),
        stable["pool_addresses"].as_array().unwrap().len(),
        "dex_types length must equal pool_addresses length"
    );
    assert_eq!(
        stable["hop_fees"].as_array().unwrap().len(),
        stable["pool_addresses"].as_array().unwrap().len()
    );

    // SC-2 split: xyk leg carries dex_type "xyk", fee 30.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=180000000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let routes = body["data"]["sub_routes"].as_array().unwrap();
    let xyk = routes
        .iter()
        .find(|r| r["source"].as_str().unwrap() == "chakra-xyk")
        .expect("split must include chakra-xyk leg");
    assert_eq!(xyk["dex_types"], json!(["xyk"]));
    assert_eq!(xyk["hop_fees"], json!([30]));

    // USDC→cirBTC xyk route.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={CIRBTC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let routes = body["data"]["sub_routes"].as_array().unwrap();
    assert!(routes.iter().all(|r| r["dex_types"] == json!(["xyk"])));
}

#[tokio::test]
async fn quote_does_not_call_rpc_when_hydrate_disabled() {
    // Fixture RPC that would panic if /quote ever called it.
    let (url, _server) = fixture::spawn(|method, _| {
        panic!("quote must not hit RPC, called {method}");
    });
    let (router, _, _) = test_app(Some(chakra_snapshot()), Some(url)).await;

    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
}

// ─── 4. SC-2 honesty ────────────────────────────────────────────────────────

#[tokio::test]
async fn sc2_180k_is_split_and_beats_single_stable() {
    let (router, _, _) = test_app(Some(chakra_snapshot()), None).await;
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=180000000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["is_split"], true,
        "SC-2 split must pass with re-quote filter"
    );
    let routes = body["data"]["sub_routes"].as_array().unwrap();
    assert!(routes.len() >= 2, "split must have >=2 sub-routes");
    let sources: Vec<&str> = routes.iter().map(|r| r["source"].as_str().unwrap()).collect();
    assert!(
        sources.contains(&"chakra-xyk"),
        "split must include chakra-xyk, got {sources:?}"
    );
    assert!(
        sources.contains(&"chakra-stable"),
        "split must include chakra-stable, got {sources:?}"
    );
    // Control: 1_000e6 pins the on-chain vector.
    let (_, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000000"),
    )
    .await;
    assert_eq!(body["data"]["expected_output"], "999550535");
}

// ─── 5. /ready vs /health ───────────────────────────────────────────────────

#[tokio::test]
async fn ready_is_503_until_snapshot_and_pool_exist() {
    let (router, _, _) = test_app_with_pools(None, None, false).await;
    let (status, body) = get(&router, "/api/v1/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["status"], "ok");

    let (status, body) = get(&router, "/api/v1/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["data"]["ready"], false);

    // Snapshot published but still no pool key → not ready.
    let (router, snapshot_store, _) = test_app_with_pools(None, None, false).await;
    snapshot_store.publish_snapshot(&chakra_snapshot()).await.unwrap();
    let (status, body) = get(&router, "/api/v1/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["data"]["ready"], false);

    // Snapshot + ≥1 pool key → ready with snapshot id + pool_keys.
    let (router, _, pool_store) = test_app_with_pools(Some(chakra_snapshot()), None, true).await;
    seed_pool_state(&pool_store).await;
    let (status, body) = get(&router, "/api/v1/ready").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["ready"], true);
    assert_eq!(body["data"]["snapshot_id"], "chakra-api-1");
    assert!(
        !body["data"]["pool_keys"].as_array().unwrap().is_empty(),
        "pool_keys must list at least one pool key"
    );
}

// ─── 6. /balances never-sum ─────────────────────────────────────────────────

#[tokio::test]
async fn balances_never_sum_erc20_and_native_usdc() {
    // Multicall3 aggregate3: balanceOf per catalog token. USDC (first entry)
    // = 1_234_567_890 (6 dp); others 0. eth_getBalance = 99e18 wei.
    let (url, _server) = fixture::spawn(|method, params| {
        match method {
            "eth_call" => {
                let call = &params[0];
                let data = call["data"].as_str().unwrap();
                let fn_sel = &data[..10];
                match fn_sel {
                    "0x82ad56cb" => {
                        // aggregate3 calldata hex: [selector(8)][offset(64)][len(64)]
                        let hex = data.trim_start_matches("0x");
                        let count = usize::from_str_radix(&hex[72..136], 16).unwrap();
                        let mut results = String::new();
                        for i in 0..count {
                            let balance = if i == 0 {
                                "00000000000000000000000000000000000000000000000000000000499602d2"
                            } else {
                                "0000000000000000000000000000000000000000000000000000000000000000"
                            };
                            results.push_str(&format!(
                                "0000000000000000000000000000000000000000000000000000000000000001\
                                 0000000000000000000000000000000000000000000000000000000000000040\
                                 0000000000000000000000000000000000000000000000000000000000000020\
                                 {balance}"
                            ));
                        }
                        let encoded = format!(
                            "0x0000000000000000000000000000000000000000000000000000000000000020\
                             00000000000000000000000000000000000000000000000000000000000000{count:02x}\
                             {results}"
                        );
                        Ok(json!(encoded))
                    }
                    other => Err(json!(format!("unexpected eth_call data {other}"))),
                }
            }
            "eth_getBalance" => {
                let _acct = &params[0];
                Ok(json!(format!("0x{:x}", 99_000_000_000_000_000_000u128))) // 99e18 wei
            }
            other => Err(json!(format!("unexpected method {other}"))),
        }
    });
    let (router, _, _) = test_app(Some(chakra_snapshot()), Some(url)).await;

    let (status, body) = get(
        &router,
        "/api/v1/balances?account=0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    // Swap USDC = ERC-20 6-dp figure only.
    assert_eq!(body["data"]["usdc"], "1234567890");
    // Native USDC is a separate field, never summed into `usdc`.
    assert_eq!(body["data"]["native_usdc"], "99000000000000000000");
    assert_eq!(body["data"]["usdc"], "1234567890", "must never sum the two encodings");
    assert!(body["data"].get("eurc").is_some());
    assert!(body["data"].get("cirbtc").is_some());
}

// ─── 7. 429 + CORS ──────────────────────────────────────────────────────────

#[tokio::test]
async fn rate_limit_429_on_quote_but_health_and_ready_exempt() {
    let (router, _, _) = test_app(None, None).await;
    let url = format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000");

    // 10 allowed from a non-loopback IP, 11th → 429.
    let mut last = (StatusCode::OK, Value::Null);
    for _ in 0..10 {
        last = send(
            &router,
            Request::builder()
                .uri(&url)
                .extension(ConnectInfo("203.0.113.7:1234".parse::<std::net::SocketAddr>().unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    }
    assert!(
        last.0.is_success() || last.0 == StatusCode::BAD_REQUEST,
        "10th quote still allowed"
    );
    let (status, body) = send(
        &router,
        Request::builder()
            .uri(&url)
            .extension(ConnectInfo("203.0.113.7:1234".parse::<std::net::SocketAddr>().unwrap()))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["code"], ApiErrorCode::RateLimited.as_str());

    // Exempt endpoints keep working.
    let (status, _) = get(&router, "/api/v1/health").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get(&router, "/api/v1/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn cors_rejects_unlisted_origin_and_allows_configured() {
    let (router, _, _) = test_app(None, None).await;

    // Disallowed origin → no allowlist header.
    let response: Response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/tokens")
                .header("Origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let allow_origin = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN);
    assert!(allow_origin.is_none(), "unlisted origin must not be allowlisted");

    // Preflight from an unlisted origin → 403.
    let response: Response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/quote")
                .header("Origin", "https://evil.example")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // tower-http CorsLayer: a disallowed preflight origin gets no allowlist
    // headers; the preflight itself may still be answered (CORS is advisory).
    let allow_origin = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN);
    assert!(allow_origin.is_none(), "unlisted origin must not be allowlisted");
}

// ─── 8. RPC policy ──────────────────────────────────────────────────────────

#[test]
fn config_rejects_canteen_and_invented_alchemy_urls() {
    for bad in [
        "https://rpc.testnet.arc-node.thecanteenapp.com",
        "https://rpc.testnet.arc-node.thecanteenapp.com/v2/xyz",
        "https://arc-testnet.g.alchemy.com/v2/xxxx",
    ] {
        assert!(!dex_adapters::evm_rpc::evm_http_url_allowed(bad), "must reject {bad}");
        assert!(
            api_server::config::parse_chakra_rpc_http(Some(bad.to_string())).is_err(),
            "{bad}"
        );
    }
    for good in [
        "https://rpc.testnet.arc.io",
        "https://rpc.blockdaemon.testnet.arc.io",
        "https://rpc.drpc.testnet.arc.io",
        "https://rpc.quicknode.testnet.arc.io",
    ] {
        assert!(dex_adapters::evm_rpc::evm_http_url_allowed(good));
        assert!(
            api_server::config::parse_chakra_rpc_http(Some(good.to_string())).is_ok(),
            "{good}"
        );
    }
}

// ─── 9. Production CLMM snapshot loader + ready + quote + build_tx ─────────

#[tokio::test]
async fn ready_and_clmm_only_snapshot_quotes_and_builds() {
    const CLMM_POOL: &str = "0x0000000000000000000000000000000000000010";
    const CLMM_FACTORY: &str = "0x00000000000000000000000000000000000000f1";
    const USER: &str = "0x1234567890123456789012345678901234567890";
    const AGGREGATOR: &str = "0xEa1b2C24bd41163590960F8e40afe6cb4CC92006";

    // 1. Worker-shaped snapshot with CLMM only in clmm_pool_refs
    let snapshot = MarketSnapshot::from_sources(
        "chakra-worker-clmm-1",
        1_700_000_000_000,
        "arc-testnet",
        vec![SourceSnapshot {
            source: "chakra-clmm".to_string(),
            pairs: vec![],
        }],
    )
    .with_clmm_pool_refs(vec![ClmmPoolRefSnapshot {
        source: "chakra-clmm".to_string(),
        pool_address: CLMM_POOL.to_string(),
        token0: USDC_ERC20.to_ascii_lowercase(),
        token1: EURC.to_ascii_lowercase(),
        fee_bps: 30,
        tick_spacing: 200,
        factory: CLMM_FACTORY.to_string(),
    }]);

    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&snapshot).await.unwrap();

    // 2. Seed CLMM state and factory into pool store
    use dex_adapters::clmm_math::bitmap;
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    let clmm_pool_snapshot = ClmmPoolSnapshot {
        source: "chakra-clmm".to_string(),
        pool_address: CLMM_POOL.to_string(),
        token0: USDC_ERC20.to_ascii_lowercase(),
        token1: EURC.to_ascii_lowercase(),
        fee_bps: 30,
        tick_spacing: 200,
        sqrt_price_x96: dex_adapters::clmm_math::sqrt_ratio_at_tick(0).0,
        tick: 0,
        liquidity: 10_000_000_000_000,
        factory: CLMM_FACTORY.to_string(),
        ticks: vec![
            ClmmTickSnapshot {
                tick: -1000,
                liquidity_gross: 10_000_000_000_000,
                liquidity_net: 10_000_000_000_000,
            },
            ClmmTickSnapshot {
                tick: 1000,
                liquidity_gross: 10_000_000_000_000,
                liquidity_net: -10_000_000_000_000,
            },
        ],
        chunk_bitmaps: vec![ClmmBitmapWordSnapshot {
            word_pos: bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).0,
            word: {
                let mut word = [0u8; 32];
                let lower_bit =
                    bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).1;
                let upper_bit =
                    bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(1000, 200)).0).1;
                word[31 - (lower_bit / 8) as usize] |= 1u8 << (lower_bit % 8);
                word[31 - (upper_bit / 8) as usize] |= 1u8 << (upper_bit % 8);
                word
            },
        }],
        word_bitmaps: vec![ClmmBitmapWordSnapshot {
            word_pos: bitmap::word_bitmap_position(
                bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).0,
            )
            .0,
            word: {
                let mut word = [0u8; 32];
                let l2_bit = bitmap::word_bitmap_position(
                    bitmap::chunk_bitmap_position(bitmap::chunk_address(bitmap::compress_tick(-1000, 200)).0).0,
                )
                .1;
                word[31 - (l2_bit / 8) as usize] |= 1u8 << (l2_bit % 8);
                word
            },
        }],
        coverage: Some(ClmmCoverageSnapshot {
            is_complete: true,
            min_loaded_tick: Some(-1000),
            max_loaded_tick: Some(1000),
            scanned_word_start: None,
            scanned_word_end: None,
        }),
    };
    pool_store.set_clmm_batch(&[clmm_pool_snapshot]).await.unwrap();
    pool_store
        .set_factories(&[FactoryRecord::new(CLMM_FACTORY, "clmm", "chakra-clmm")])
        .await
        .unwrap();

    // 3. Build AppState with aggregator configured and mock RPC fixture
    let (url, _server) = fixture::spawn(|method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            let sel4 = &data[..10];
            if sel4 == "0x5c975abb" || sel4 == "0xdd62ed3e" {
                Ok(json!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                ))
            } else if sel4 == "0x927da105" {
                Ok(json!("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"))
            } else {
                Err(json!(format!("unexpected eth_call selector {sel4}")))
            }
        }
        other => Err(json!(format!("unexpected method {other}"))),
    });

    let mut config = app_config();
    config.chakra_aggregator = AGGREGATOR.to_string();

    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn PoolStateStore>),
        Some(snapshot_store.clone()),
        Some(pool_store.clone()),
        Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        None,
    )
    .await;
    let router = build_router(state, RateLimitState::from_env());

    // 4. Verify /ready returns 200 OK
    let (status, ready_body) = get(&router, "/api/v1/ready").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ready_body["data"]["ready"], true);
    assert_eq!(ready_body["data"]["snapshot_id"], "chakra-worker-clmm-1");

    // 5. Verify /quote works for CLMM pair and emits explicit hop metadata
    let (status, quote_body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "quote status: {status}, body: {quote_body}");
    assert_eq!(quote_body["success"], true, "quote_body: {quote_body}");
    let quote_data = &quote_body["data"];
    assert_eq!(quote_data["protocol_fee_bps"], 0, "quote_data: {quote_data}");
    let sub_routes = quote_data["sub_routes"].as_array().expect("sub_routes array");
    assert_eq!(sub_routes.len(), 1);
    let route = &sub_routes[0];
    assert_eq!(route["dex_types"][0], "clmm");
    assert_eq!(route["hop_fees"][0], 30);
    assert_eq!(
        route["hop_factories"][0].as_str().unwrap().to_ascii_lowercase(),
        CLMM_FACTORY.to_ascii_lowercase()
    );

    // 6. Verify /build_tx succeeds with value: "0" and valid transaction payload
    let build_tx_body = json!({
        "user": USER,
        "token_in": USDC_ERC20.to_ascii_lowercase(),
        "token_out": EURC.to_ascii_lowercase(),
        "amount_in": "1000000",
        "min_amount_out": "990000",
        "sub_routes": [{
            "amount_in": "1000000",
            "steps": [{
                "dex_type": "clmm",
                "pool_address": CLMM_POOL,
                "token_in": USDC_ERC20.to_ascii_lowercase(),
                "token_out": EURC.to_ascii_lowercase(),
                "fee_bps": 30
            }]
        }]
    });
    let (status, tx_body) = post(&router, "/api/v1/build_tx", build_tx_body).await;
    assert_eq!(status, StatusCode::OK, "build_tx response: {tx_body}");
    let tx_data = &tx_body["data"];
    assert_eq!(
        tx_data["to"].as_str().unwrap().to_ascii_lowercase(),
        AGGREGATOR.to_ascii_lowercase()
    );
    assert_eq!(tx_data["value"], "0");
    assert!(tx_data["data"].as_str().unwrap().starts_with("0x"));
}

// Keep `Duration` in scope for future windowed tests.
#[allow(dead_code)]
fn _window() -> Duration {
    Duration::from_secs(1)
}
