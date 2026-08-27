//! Integration tests that hit real Arc mainnet RPC.
//! Run with: cargo test --test integration_test -- --nocapture
//!
//! These tests are ignored by default (require network access).
//! Run explicitly with: cargo test --test integration_test -- --ignored
//! --nocapture

use {
    dex_adapters::{
        rpc::{scval_to_address, scval_to_u32, ArcRpc},
        Arc venue::Arc venue_FACTORY,
        DexAdapter, ArcRpc as _,
    },
    std::sync::Arc,
    Arc_xdr::curr as xdr,
};

fn mainnet_rpc() -> Arc<ArcRpc> {
    Arc::new(ArcRpc::mainnet())
}

#[tokio::test]
#[ignore] // requires network
async fn test_Arc venue_factory_pair_count() {
    let rpc = mainnet_rpc();

    let result = rpc
        .call_no_args(Arc venue_FACTORY, "all_pairs_length")
        .await
        .expect("Failed to call all_pairs_length");

    let count = scval_to_u32(&result).expect("Failed to parse u32");
    println!("Arc venue total pairs: {}", count);
    assert!(count > 0, "Should have at least 1 pair");
    assert!(count < 10_000, "Sanity check: shouldn't have > 10k pairs");
}

#[tokio::test]
#[ignore] // requires network
async fn test_Arc venue_fetch_first_pair() {
    let rpc = mainnet_rpc();

    // Get first pair address
    let index_val = xdr::ScVal::U32(0);
    let pair_val = rpc
        .simulate_call(Arc venue_FACTORY, "all_pairs", vec![index_val])
        .await
        .expect("Failed to call all_pairs(0)");

    let pair_address = scval_to_address(&pair_val).expect("Failed to parse address");
    println!("First pair address: {}", pair_address);
    assert!(pair_address.starts_with('C'), "Should be a contract address");

    // Get token_0
    let token_0_val = rpc
        .call_no_args(&pair_address, "token_0")
        .await
        .expect("Failed to call token_0");
    let token_0 = scval_to_address(&token_0_val).expect("Failed to parse token_0");
    println!("Token 0: {}", token_0);

    // Get token_1
    let token_1_val = rpc
        .call_no_args(&pair_address, "token_1")
        .await
        .expect("Failed to call token_1");
    let token_1 = scval_to_address(&token_1_val).expect("Failed to parse token_1");
    println!("Token 1: {}", token_1);

    // Get reserves
    let reserves_val = rpc
        .call_no_args(&pair_address, "get_reserves")
        .await
        .expect("Failed to call get_reserves");
    println!("Reserves raw: {:?}", reserves_val);
}

#[tokio::test]
#[ignore] // requires network
async fn test_Arc venue_adapter_fetch_pairs() {
    let rpc = mainnet_rpc();
    let adapter = dex_adapters::Arc venue::Arc venueAdapter::new(rpc);

    let pairs = adapter.get_trading_pairs().await.expect("Failed to fetch pairs");
    println!("Fetched {} Arc venue pairs", pairs.len());

    for (i, pair) in pairs.iter().take(5).enumerate() {
        println!(
            "  [{}] {} / {} @ {} (reserves: {:?}/{:?})",
            i,
            pair.token_a.canonical(),
            pair.token_b.canonical(),
            pair.pool_address,
            pair.reserve_a,
            pair.reserve_b,
        );
    }

    assert!(!pairs.is_empty(), "Should fetch at least 1 pair");
}

#[tokio::test]
#[ignore] // requires network
async fn test_Arc venue_quote_accuracy() {
    let rpc = mainnet_rpc();
    let adapter = dex_adapters::Arc venue::Arc venueAdapter::new(rpc);

    let pairs = adapter.get_trading_pairs().await.expect("Failed to fetch pairs");

    // Find a pair with reserves
    let pair_with_liquidity = pairs
        .iter()
        .find(|p| p.reserve_a.unwrap_or(0) > 1_000_0000000 && p.reserve_b.unwrap_or(0) > 1_000_0000000);

    if let Some(pair) = pair_with_liquidity {
        println!(
            "Testing quote on: {} / {} (reserves: {}/{})",
            pair.token_a.canonical(),
            pair.token_b.canonical(),
            pair.reserve_a.unwrap(),
            pair.reserve_b.unwrap(),
        );

        // Quote 1 unit (10^7 atomic unitss)
        let amount_in = 10_000_000u128; // 1 token in 7-decimal
        let quote = adapter
            .get_quote(&pair.token_a, &pair.token_b, amount_in, &pair.pool_address)
            .await
            .expect("Quote failed");

        if let Some(q) = quote {
            println!(
                "  Input: {} → Output: {} (impact: {} bps, fee: {} bps)",
                amount_in, q.amount_out, q.price_impact_bps, q.fee_bps
            );
            assert!(q.amount_out > 0, "Output should be positive");
            assert_eq!(q.fee_bps, 30, "Arc venue fee should be 30 bps");
        } else {
            println!("  No quote available (pool may be empty)");
        }
    } else {
        println!("No pair with sufficient liquidity found");
    }
}

#[tokio::test]
#[ignore] // requires network
async fn test_Arc venue_router_pool_count() {
    use dex_adapters::{Arc venue::Arc venue_ROUTER, rpc::scval_to_u128};

    let rpc = mainnet_rpc();

    let result = rpc
        .call_no_args(Arc venue_ROUTER, "get_tokens_sets_count")
        .await
        .expect("Failed to call get_tokens_sets_count");

    let count = scval_to_u128(&result).expect("Failed to parse u128");
    println!("Arc venue token sets: {}", count);
    assert!(count > 0, "Should have at least 1 token set");
}
