//! Local encode-only API harness for T7.2 / SC-6.
//!
//! Starts an in-memory Chakra API on `CHAKRA_LISTEN_ADDR` (default
//! `127.0.0.1:8080`) with fixture RPC and seeded fixture pools. The SDK
//! example `packages/sdk/examples/quote-build.ts` can then complete
//! `quote` + `build_tx` against this server.
//!
//! Usage:
//!   cargo run -p api-server --example local_harness
//!
//! Env overrides:
//!   CHAKRA_LISTEN_ADDR  — bind address (default 127.0.0.1:8080)
//!   CHAKRA_AGGREGATOR   — dummy encode-only address (default 0xaa…aa)

use {
    api_server::{
        build_router,
        config::{AppConfig, ChakraMode},
        rate_limit::RateLimitState,
        state::AppState,
    },
    dex_adapters::evm_rpc::fixture,
    market_snapshot::{
        decimals::{EURC, USDC_ERC20},
        pool_state_store::{MemoryPoolStateStore, PoolStateStore, StablePoolStateValue, XykPoolStateValue},
        store::{MemorySnapshotStore, SnapshotStore},
        MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine},
    serde_json::json,
    std::sync::Arc,
};

const XYK_POOL_UE: &str = "0x0000000000000000000000000000000000000001";
const STABLE_POOL_UE: &str = "0x0000000000000000000000000000000000000002";
const XYK_UE_SEED: u128 = 10_000_000_000;
const STABLE_UE_SEED: u128 = 200_000_000_000;

fn chakra_snapshot() -> MarketSnapshot {
    MarketSnapshot::from_sources(
        "chakra-local-1",
        1_700_000_000_000,
        "arc-testnet",
        vec![
            SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: USDC_ERC20.to_string(),
                    token_b: EURC.to_string(),
                    pool_address: XYK_POOL_UE.to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                }],
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

fn app_config(aggregator: &str) -> AppConfig {
    let mut config = AppConfig::default();
    config.Chakra_mode = ChakraMode::Embedded;
    config.snapshot_backend = Some("memory".to_string());
    config.chakra_aggregator = aggregator.to_string();
    config
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let listen_addr = std::env::var("CHAKRA_LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let aggregator =
        std::env::var("CHAKRA_AGGREGATOR").unwrap_or_else(|_| "0x00000000000000000000000000000000000000aa".to_string());

    // 1. Fixture RPC: paused=false, ERC-20→Permit2=0, Permit2→aggregator=0
    //    (typed data required for build_tx).
    let (rpc_url, _rpc_handle) = fixture::spawn(|method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            match &data[..10] {
                // paused() → false
                "0x5c975abb" => Ok(json!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                )),
                // ERC-20 allowance(user → Permit2) → 0 (approval required)
                "0xdd62ed3e" => Ok(json!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                )),
                // Permit2 allowance(user, tokenIn → aggregator) → 0 (typed data required)
                "0x927da105" => Ok(json!(
                    "0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
                )),
                other => Err(json!(format!("unexpected eth_call selector {other}"))),
            }
        }
        other => Err(json!(format!("unexpected RPC method {other}"))),
    });
    eprintln!("[harness] fixture RPC: {rpc_url}");

    // 2. Snapshot + pool stores.
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&chakra_snapshot()).await.unwrap();

    let pool_store = Arc::new(MemoryPoolStateStore::new());
    let eurc_lower = EURC.to_lowercase();
    pool_store
        .set_xyk_batch(&[XykPoolStateValue::new(
            "chakra-xyk",
            XYK_POOL_UE,
            USDC_ERC20,
            &eurc_lower,
            30,
            XYK_UE_SEED,
            XYK_UE_SEED,
        )])
        .await
        .unwrap();
    pool_store
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

    // 3. QuoteEngine.
    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&chakra_snapshot()).await;

    // 4. AppState + router.
    let config = app_config(&aggregator);
    let state = AppState::from_backends(
        config,
        None,
        None,
        Some(snapshot_store),
        Some(pool_store),
        Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&rpc_url).unwrap())),
        Some(("fixture-v1".to_string(), Arc::new(engine))),
    )
    .await;
    let router = build_router(state, RateLimitState::from_env());

    eprintln!("[harness] Chakra API listening on http://{listen_addr}");
    eprintln!("[harness] aggregator = {aggregator}");
    eprintln!("[harness] tokens:");
    eprintln!("  USDC = {USDC_ERC20}");
    eprintln!("  EURC = {EURC}");
    eprintln!("[harness] try:");
    eprintln!("  curl -s http://{listen_addr}/api/v1/health | jq .");
    eprintln!("  curl -s 'http://{listen_addr}/api/v1/quote?token_in={USDC_ERC20}&token_out={EURC}&amount_in=100000000' | jq .");

    let listener = tokio::net::TcpListener::bind(&listen_addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
