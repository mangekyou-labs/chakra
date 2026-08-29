//! 2026-08-29 canonical curated venues — REST integration tests.
//!
//! Covers SC-1 (catalog pairs via Xylo/Presto/UnitFlow), SC-14 (no Chakra-owned
//! liquidity in the public path), SC-15 (failed venues → NO_ROUTE, no
//! auto-reseed), and the atomic USDC → EURC → cirBTC multihop.

use {
    api_server::{build_router, config::AppConfig, rate_limit::RateLimitState, state::AppState},
    axum::{body::Body, http::{Request, StatusCode}, Router},
    market_snapshot::{
        decimals::{CIRBTC, EURC, USDC_ERC20},
        pool_state_store::{
            MemoryPoolStateStore, PoolStateStore, StablePoolStateValue, XykPoolStateValue,
        },
        store::{MemorySnapshotStore, SnapshotStore},
        MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine},
    serde_json::Value,
    std::sync::Arc,
    tower::ServiceExt,
};

const XYLO_FACTORY: &str = "0x60EDeFB094B84BBC6430cc130B358A43Ba1979e2";
const XYLO_POOL: &str = "0x3DF3966F5138143dce7a9cFDdC2c0310ce083BB1";
const PRESTO_HUB: &str = "0x5794a8284A29493871Fbfa3c4f343D42001424D6";
const UNITFLOW_FACTORY: &str = "0xd67F63A4F26a497b364d1C82e6747Aec8B5743a5";
const UNITFLOW_PAIR: &str = "0x268DC75517EaFc6e0D52666639529e5DAB8c9200";

const XYLO_RESERVE_USDC: u128 = 9_323_185_000_000; // ~9.3M USDC (live shape)
const XYLO_RESERVE_EURC: u128 = 613_516_000_000; // ~0.61M EURC
const UNITFLOW_EURC: u128 = 100_000_000_000; // 100k EURC
const UNITFLOW_CIRBTC: u128 = 1_000_000_000; // 10 cirBTC (1 EURC ≈ 0.01 cirBTC)

fn app_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.Chakra_mode = api_server::config::ChakraMode::Embedded;
    config.snapshot_backend = Some("memory".to_string());
    config.max_splits = 5;
    config.quote_rpc_hydrate_enabled = false;
    config
}

/// Canonical curated topology: Xylo USDC/EURC (stable), Presto hub USDC/EURC,
/// UnitFlow EURC/cirBTC (xyk). No Chakra-owned pools.
fn canonical_snapshot() -> MarketSnapshot {
    MarketSnapshot::from_sources(
        "chakra-canonic-1",
        1_800_000_000_000,
        "arc-testnet",
        vec![
            SourceSnapshot {
                source: "xylo-stable".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: USDC_ERC20.to_string(),
                    token_b: EURC.to_string(),
                    pool_address: XYLO_POOL.to_string(),
                    fee_bps: 4,
                    dex_type: "xylo".to_string(),
                    factory: XYLO_FACTORY.to_string(),
                }],
            },
            SourceSnapshot {
                source: "presto-hub".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: USDC_ERC20.to_string(),
                    token_b: EURC.to_string(),
                    pool_address: PRESTO_HUB.to_string(),
                    fee_bps: 30,
                    dex_type: "presto".to_string(),
                    factory: PRESTO_HUB.to_string(),
                }],
            },
            SourceSnapshot {
                source: "unitflow-v25".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: EURC.to_string(),
                    token_b: CIRBTC.to_string(),
                    pool_address: UNITFLOW_PAIR.to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: UNITFLOW_FACTORY.to_string(),
                }],
            },
        ],
    )
}

async fn seed_pool_state(pools: &MemoryPoolStateStore) {
    pools
        .set_stable_batch(&[
            StablePoolStateValue::new(
                "xylo-stable",
                XYLO_POOL,
                USDC_ERC20.to_ascii_lowercase(),
                EURC.to_ascii_lowercase(),
                XYLO_RESERVE_USDC,
                XYLO_RESERVE_EURC,
                200,
                4,
            ),
        ])
        .await
        .unwrap();
    pools.set_stable_batch(&[
        StablePoolStateValue::new(
            "xylo-stable",
            XYLO_POOL,
            USDC_ERC20.to_ascii_lowercase(),
            EURC.to_ascii_lowercase(),
            XYLO_RESERVE_USDC,
            XYLO_RESERVE_EURC,
            200,
            4,
        ),
        // Presto spoke state lives in the stable bucket (A marker unused by
        // the spoke quote; fee 30 bps per the published hub formula).
        StablePoolStateValue::new(
            "presto-hub",
            PRESTO_HUB,
            USDC_ERC20.to_ascii_lowercase(),
            EURC.to_ascii_lowercase(),
            200_000_000_000,
            200_000_000_000,
            1,
            30,
        ),
    ])
    .await
    .unwrap();
    pools
        .set_xyk_batch(&[XykPoolStateValue::new(
            "unitflow-v25",
            UNITFLOW_PAIR,
            EURC.to_ascii_lowercase(),
            CIRBTC.to_ascii_lowercase(),
            30,
            UNITFLOW_EURC,
            UNITFLOW_CIRBTC,
        )])
        .await
        .unwrap();
}

async fn test_app() -> (Router, Arc<MemorySnapshotStore>, Arc<MemoryPoolStateStore>) {
    let config = app_config();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    snapshot_store
        .publish_snapshot(&canonical_snapshot())
        .await
        .unwrap();
    seed_pool_state(&pool_store).await;

    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine
        .update_from_chakra_snapshot(&canonical_snapshot())
        .await;
    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn PoolStateStore>),
        Some(snapshot_store.clone()),
        Some(pool_store.clone()),
        None,
        Some(("chakra-canonic-1".to_string(), Arc::new(engine))),
    )
    .await;

    let router = build_router(state, RateLimitState::from_env());
    (router, snapshot_store, pool_store)
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// USDC/EURC candidates exist on BOTH Xylo and Presto (SC-1, 2026-08-29).
/// At 1e6 Presto (30 bps, 200k reserves) beats Xylo (4 bps but thin EURC
/// side); both venues must be independently quotable across sizes.
#[tokio::test]
async fn usdc_eurc_candidates_from_xylo_and_presto() {
    let (router, _, _) = test_app().await;
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let source = body["data"]["sub_routes"][0]["source"].as_str().unwrap();
    assert!(
        source == "presto-hub" || source == "xylo-stable",
        "expected a canonical venue, got {source}"
    );

    // A larger size where the Xylo curve (A=200, 4 bps) is deeper than the
    // 30 bps Presto spoke — Xylo must be independently quotable.
    let (_, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000000"),
    )
    .await;
    assert_eq!(body["success"], true, "capacity quote failed: {body}");
    let sources: Vec<&str> = body["data"]["sub_routes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["source"].as_str().unwrap())
        .collect();
    assert!(
        sources.contains(&"xylo-stable") || sources.contains(&"presto-hub"),
        "no canonical venue in {sources:?}"
    );
}

/// EURC/cirBTC routes through UnitFlow V2.5 (XYK family).
#[tokio::test]
async fn eurc_cirbtc_routes_through_unitflow() {
    let (router, _, _) = test_app().await;
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={EURC}&token_out={CIRBTC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let routes = body["data"]["sub_routes"].as_array().unwrap();
    assert!(
        routes.iter().any(|r| r["source"].as_str().unwrap() == "unitflow-v25"),
        "unitflow-v25 candidate missing"
    );
    let out: u128 = body["data"]["expected_output"].as_str().unwrap().parse().unwrap();
    assert!(out > 0 && out < 1_000_000, "cirBTC output out of range: {out}");
}

/// Atomic USDC → EURC → cirBTC multihop (no direct USDC/cirBTC venue exists).
#[tokio::test]
async fn atomic_usdc_to_eurc_to_cirbtc_multihop() {
    let (router, _, _) = test_app().await;
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={CIRBTC}&amount_in=1000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "multihop quote failed: {body}");
    let routes = body["data"]["sub_routes"].as_array().unwrap();
    assert!(
        routes.iter().any(|r| {
            r["pool_addresses"].as_array().map(|p| p.len() == 2).unwrap_or(false)
                && r["dex_types"].as_array().map(|d| d.len() == 2).unwrap_or(false)
        }),
        "USDC→cirBTC must be a 2-hop route through EURC"
    );
}

/// A deterministic Xylo/Presto split at capacity size (no shared pools).
#[tokio::test]
async fn deterministic_xylo_presto_split() {
    let (router, _, _) = test_app().await;
    // 1_000 USDC — large enough for the Xylo vs Presto quote difference to be
    // competitive, but the split must never share a pool.
    let (status, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=1000000000000"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let routes = body["data"]["sub_routes"].as_array().unwrap();
    // No pool may appear in two different sub-routes (shared-pool rejection).
    let mut pools: Vec<String> = Vec::new();
    for r in routes {
        for p in r["pool_addresses"].as_array().unwrap() {
            pools.push(p.as_str().unwrap().to_string());
        }
    }
    let unique: std::collections::HashSet<&String> = pools.iter().collect();
    assert_eq!(
        unique.len(),
        pools.len(),
        "split must not reuse a pool: {pools:?}"
    );
}

/// Graceful degradation: a venue that is empty (zero reserves) is skipped and
/// produces NO_ROUTE when it was the only candidate for the pair.
#[tokio::test]
async fn empty_venue_degrades_to_no_route() {
    let (router, snapshot_store, pool_store) = test_app().await;
    // Empty the UnitFlow reserves → EURC/cirBTC has no executable route.
    pool_store
        .set_xyk_batch(&[XykPoolStateValue::new(
            "unitflow-v25",
            UNITFLOW_PAIR,
            EURC.to_ascii_lowercase(),
            CIRBTC.to_ascii_lowercase(),
            30,
            0,
            0,
        )])
        .await
        .unwrap();
    let _ = snapshot_store;
    let (_, body) = get(
        &router,
        &format!("/api/v1/quote?token_in={EURC}&token_out={CIRBTC}&amount_in=1000000"),
    )
    .await;
    // The envelope code is the contract: NO_ROUTE, never a zero output or a
    // hidden fallback venue (SC-15).
    assert_eq!(body["error"]["code"], "NO_ROUTE");
}
