//! T4.4 `/build_tx` splitSwap calldata encoder integration tests.
//!
//! The encoder validates continuity / amount sum / snapshot + factory
//! membership and never re-quotes. RPC calls (fixture): `paused()`,
//! ERC-20 `allowance(from, Permit2)`, Permit2 `allowance(from, tokenIn,
//! aggregator)`. `typedData` omitted when the Permit2 allowance is sufficient;
//! `required_approvals` empty when the ERC-20 allowance to Permit2 suffices.

use {
    api_server::{
        build_router, config::AppConfig, envelope::ApiErrorCode, rate_limit::RateLimitState, state::AppState,
    },
    axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    },
    dex_adapters::evm_rpc::fixture,
    market_snapshot::{
        decimals::{EURC, USDC_ERC20},
        pool_state_store::{MemoryPoolStateStore, PoolStateStore, StablePoolStateValue, XykPoolStateValue},
        store::{MemorySnapshotStore, SnapshotStore},
        MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    router_engine::{path_finder::PathFinderConfig, split_optimizer::SplitConfig, QuoteEngine},
    serde_json::{json, Value},
    std::sync::Arc,
    tower::ServiceExt,
};

#[test]
fn split_swap_calldata_matches_solidity_abi_for_nested_routes() {
    let routes = vec![api_server::handlers::BuildTxSubRoute {
        amount_in: "100".to_string(),
        steps: vec![api_server::handlers::BuildTxStep {
            pool_address: "0x0000000000000000000000000000000000000003".to_string(),
            dex_type: "xyk".to_string(),
            token_in: "0x0000000000000000000000000000000000000001".to_string(),
            token_out: "0x0000000000000000000000000000000000000002".to_string(),
            fee_bps: None,
        }],
    }];

    let encoded = api_server::build_tx::encode_split_swap(
        "0x0000000000000000000000000000000000000001",
        "0x0000000000000000000000000000000000000002",
        100,
        90,
        12_345,
        &routes,
        &[],
        None,
    )
    .unwrap();

    // Canonical fixture generated with Foundry `cast calldata` for the
    // Aggregator.splitSwap Solidity signature. This catches offsets inside
    // SubRoute[] and the static Hop[] tuple layout.
    let expected = concat!(
        "0x2e3be0c10000000000000000000000000000000000000000000000000000000000000001",
        "0000000000000000000000000000000000000000000000000000000000000002",
        "0000000000000000000000000000000000000000000000000000000000000064",
        "000000000000000000000000000000000000000000000000000000000000005a",
        "0000000000000000000000000000000000000000000000000000000000003039",
        "00000000000000000000000000000000000000000000000000000000000000e0",
        "0000000000000000000000000000000000000000000000000000000000000220",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "0000000000000000000000000000000000000000000000000000000000000020",
        "0000000000000000000000000000000000000000000000000000000000000064",
        "0000000000000000000000000000000000000000000000000000000000000040",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "0000000000000000000000000000000000000000000000000000000000000003",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "0000000000000000000000000000000000000000000000000000000000000002",
        "000000000000000000000000000000000000000000000000000000000000001e",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "00000000000000000000000000000000000000000000000000000000000000e0",
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert_eq!(encoded, expected);
}

const MBTC: &str = "0x1111111111111111111111111111111111111111";
const XYK_POOL_UE: &str = "0x0000000000000000000000000000000000000001";
const STABLE_POOL_UE: &str = "0x0000000000000000000000000000000000000002";
const AGGREGATOR: &str = "0x00000000000000000000000000000000000000aa";
const PERMIT2: &str = "0x000000000022D473030F116dDEE9F6B43aC78BA3";
const USER: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const SPLIT_SWAP_SELECTOR: &str = "2e3be0c1";

fn app_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.lumagg_mode = api_server::config::LumaggMode::Embedded;
    config.snapshot_backend = Some("memory".to_string());
    config.chakra_aggregator = AGGREGATOR.to_string();
    config
}

fn chakra_snapshot() -> MarketSnapshot {
    MarketSnapshot::from_sources(
        "chakra-build-1",
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

async fn seed_pools(pools: &MemoryPoolStateStore) {
    pools
        .set_xyk_batch(&[XykPoolStateValue::new(
            "chakra-xyk",
            XYK_POOL_UE,
            USDC_ERC20,
            EURC,
            30,
            10_000_000_000,
            10_000_000_000,
        )])
        .await
        .unwrap();
    pools
        .set_stable_batch(&[StablePoolStateValue::new(
            "chakra-stable",
            STABLE_POOL_UE,
            USDC_ERC20,
            EURC,
            200_000_000_000,
            200_000_000_000,
            100,
            4,
        )])
        .await
        .unwrap();
}

/// Build a 32-byte hex word encoding a packed Permit2 allowance.
/// Packed: {amount: uint160, expiration: uint48, nonce: uint48} = 256 bits.
/// Build a 96-byte (3 × 32-byte words) hex response encoding the Permit2
/// `Allowance` struct as ABI returns: `{amount: uint160, expiration: uint48, nonce: uint48}`.
fn packed_allowance_hex(amount: u128, expiration: u64, nonce: u64) -> String {
    // 3 ABI-encoded 32-byte words:
    //   word 0: amount (uint160, right-aligned)
    //   word 1: expiration (uint48, right-aligned)
    //   word 2: nonce (uint48, right-aligned)
    let mut words = [0u8; 96];
    // amount: u128 → 16 bytes, right-aligned in the 20-byte uint160 slot (bytes 12..32)
    let amt_be = amount.to_be_bytes(); // 16 bytes
    words[16..32].copy_from_slice(&amt_be);
    // expiration: u64 → 8 bytes, right-aligned in word 1 (bytes [32+24..32+32])
    let exp_be = expiration.to_be_bytes();
    words[56..64].copy_from_slice(&exp_be);
    // nonce: u64 → 8 bytes, right-aligned in word 2 (bytes [64+24..64+32])
    let nonce_be = nonce.to_be_bytes();
    words[88..96].copy_from_slice(&nonce_be);
    format!("0x{}", hex::encode(words))
}

/// Fixture RPC: `paused()` false, ERC-20 allowance→Permit2 = 0,
/// Permit2 allowance→aggregator = 0 (typed data required).
fn permit_needed_fixture() -> (String, std::thread::JoinHandle<()>) {
    fixture::spawn(|method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            let sel4 = &data[..10];
            if sel4 == "0x5c975abb" || sel4 == "0xdd62ed3e" {
                Ok(json!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                ))
            } else if sel4 == "0x927da105" {
                // Permit2 allowance: 3 zero words (amount=0, expiration=0, nonce=0)
                Ok(json!("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"))
            } else {
                Err(json!(format!("unexpected eth_call selector {sel4}")))
            }
        }
        other => Err(json!(format!("unexpected method {other}"))),
    })
}

/// Fixture RPC with sufficient ERC-20 allowance to Permit2 (no approvals) but
/// zero Permit2 allowance (typed data still required).
fn erc20_approved_fixture() -> (String, std::thread::JoinHandle<()>) {
    fixture::spawn(|method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            let sel4 = &data[..10];
            match sel4 {
                "0x5c975abb" => Ok(json!("0x0000000000000000000000000000000000000000000000000000000000000000")),
                "0xdd62ed3e" => {
                    let to = params[0]["to"].as_str().unwrap();
                    if to.eq_ignore_ascii_case(PERMIT2) {
                        Ok(json!("0x0000000000000000000000000000000000000000000000000000000000000000"))
                    } else {
                        Ok(json!("0x00000000000000000000000000000000000000000000000000000000000f4240"))
                    }
                }
                // 3 ABI words: amount=0, expiration=0, nonce=0
                "0x927da105" => Ok(json!("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")),
                other => Err(json!(format!("unexpected eth_call selector {other}"))),
            }
        }
        other => Err(json!(format!("unexpected method {other}"))),
    })
}

/// Fixture RPC with sufficient Permit2 allowance (no typed data, no approvals).
/// Returns 3 ABI words: amount=1_000_000, expiration=far_future, nonce=0.
fn fully_approved_fixture() -> (String, std::thread::JoinHandle<()>) {
    // amount=1_000_000, expiration=far future, nonce=0
    let resp = packed_allowance_hex(1_000_000, u64::MAX, 0);
    fixture::spawn(move |method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            let sel4 = &data[..10];
            match sel4 {
                "0x5c975abb" => Ok(json!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                )),
                "0xdd62ed3e" => Ok(json!(
                    "0x00000000000000000000000000000000000000000000000000000000000f4240"
                )),
                "0x927da105" => Ok(json!(resp)),
                other => Err(json!(format!("unexpected eth_call selector {other}"))),
            }
        }
        other => Err(json!(format!("unexpected method {other}"))),
    })
}

/// Fixture RPC with `paused() = true`.
fn paused_fixture() -> (String, std::thread::JoinHandle<()>) {
    fixture::spawn(|method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            let sel4 = &data[..10];
            match sel4 {
                "0x5c975abb" => Ok(json!("0x0000000000000000000000000000000000000000000000000000000000000001")),
                "0xdd62ed3e" => Ok(json!("0x0000000000000000000000000000000000000000000000000000000000000000")),
                "0x927da105" => Ok(json!("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")),
                other => Err(json!(format!("unexpected eth_call selector {other}"))),
            }
        }
        other => Err(json!(format!("unexpected method {other}"))),
    })
}

async fn test_app(rpc_url: Option<String>) -> Router {
    let config = app_config();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&chakra_snapshot()).await.unwrap();
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    seed_pools(&pool_store).await;

    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&chakra_snapshot(), MBTC).await;

    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn market_snapshot::store::SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn market_snapshot::pool_state_store::PoolStateStore>),
        Some(snapshot_store),
        Some(pool_store),
        rpc_url.map(|url| Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        MBTC.to_string(),
        None,
    )
    .await;
    build_router(state, RateLimitState::from_env())
}

fn valid_body() -> Value {
    json!({
        "user": USER,
        "token_in": USDC_ERC20.to_ascii_lowercase(),
        "token_out": EURC.to_ascii_lowercase(),
        "amount_in": "1000000",
        "min_amount_out": "990000",
        "sub_routes": [{
            "amount_in": "1000000",
            "steps": [{
                "dex_type": "stable",
                "pool_address": STABLE_POOL_UE,
                "token_in": USDC_ERC20.to_ascii_lowercase(),
                "token_out": EURC.to_ascii_lowercase()
            }]
        }]
    })
}

async fn post(router: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn decode_hex(hex: &str) -> Vec<u8> {
    hex::decode(hex.trim_start_matches("0x")).unwrap()
}

fn word_at(bytes: &[u8], offset: usize) -> [u8; 32] {
    bytes[offset..offset + 32].try_into().unwrap()
}

fn u256_at(bytes: &[u8], offset: usize) -> u128 {
    let w = word_at(bytes, offset);
    u128::from_be_bytes(w[16..].try_into().unwrap())
}

/// Decode `splitSwap` calldata head: (tokenIn, tokenOut, amountIn,
/// minAmountOut, deadline, routesOffset, permitOffset).
struct SplitSwapHead {
    token_in: [u8; 32],
    token_out: [u8; 32],
    amount_in: u128,
    min_amount_out: u128,
    deadline: u128,
    routes_offset: usize,
    permit_offset: usize,
}

fn decode_head(data: &[u8]) -> SplitSwapHead {
    assert_eq!(
        &data[..4],
        &hex::decode(SPLIT_SWAP_SELECTOR).unwrap()[..],
        "selector mismatch"
    );
    SplitSwapHead {
        token_in: word_at(data, 4),
        token_out: word_at(data, 36),
        amount_in: u256_at(data, 68),
        min_amount_out: u256_at(data, 100),
        deadline: u256_at(data, 132),
        routes_offset: u256_at(data, 164) as usize + 4,
        permit_offset: u256_at(data, 196) as usize + 4,
    }
}

// ─── 1. Valid quote → data starts with splitSwap selector; decode matches ───

#[tokio::test]
async fn build_tx_encodes_split_swap_with_matching_route() {
    let (url, _server) = permit_needed_fixture();
    let router = test_app(Some(url)).await;
    let (status, body) = post(&router, "/api/v1/build_tx", valid_body()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);

    let data = body["data"]["data"].as_str().unwrap();
    assert!(data.starts_with("0x2e3be0c1"), "must start with splitSwap selector");
    let bytes = decode_hex(data);
    let head = decode_head(&bytes);

    assert_eq!(&head.token_in[12..], &decode_hex(&USDC_ERC20)[..]);
    assert_eq!(&head.token_out[12..], &decode_hex(&EURC.to_ascii_lowercase())[..]);
    assert_eq!(head.amount_in, 1_000_000);
    assert_eq!(head.min_amount_out, 990_000);
    assert!(head.deadline > 0, "deadline must be set (now+120s)");
    assert_eq!(body["data"]["chain_id"], 5042002);
    assert_eq!(body["data"]["value"], "0");

    // routes array: one sub-route with one stable hop.
    let routes_len = u256_at(&bytes, head.routes_offset);
    assert_eq!(routes_len, 1);
    let elem0_offset = u256_at(&bytes, head.routes_offset + 32) as usize;
    let route0 = head.routes_offset + 32 + elem0_offset;
    let sub_amount = u256_at(&bytes, route0);
    assert_eq!(sub_amount, 1_000_000);
    let hops_offset = u256_at(&bytes, route0 + 32) as usize;
    let hops_start = route0 + hops_offset;
    let hops_len = u256_at(&bytes, hops_start);
    assert_eq!(hops_len, 1);
    let hop = hops_start + 32;
    assert_eq!(&bytes[hop + 12..hop + 32], &decode_hex(&STABLE_POOL_UE)[..], "hop.pool");
    assert_eq!(u256_at(&bytes, hop + 32), 1, "hop.dexType must be Stable");
    assert_eq!(
        &bytes[hop + 64 + 12..hop + 64 + 32],
        &decode_hex(&USDC_ERC20)[..],
        "hop.tokenIn"
    );
    assert_eq!(
        &bytes[hop + 96 + 12..hop + 96 + 32],
        &decode_hex(&EURC.to_ascii_lowercase())[..],
        "hop.tokenOut"
    );
    assert_eq!(u256_at(&bytes, hop + 128), 4, "hop.fee");

    // Permit2Pull: PermitSingle words should be populated when typed_data present.
    assert!(head.permit_offset > 0);
    // token word (right-aligned address, bytes 12..32)
    let permit_token = &bytes[head.permit_offset + 12..head.permit_offset + 32];
    assert_eq!(
        permit_token,
        &decode_hex(&USDC_ERC20)[..],
        "PermitSingle.token must match token_in"
    );

    // amount word
    let permit_amount = u256_at(&bytes, head.permit_offset + 32);
    assert_eq!(permit_amount, 1_000_000, "PermitSingle.amount must match amount_in");

    // expiration word (should be deadline = now + 120s, so > 0)
    let permit_expiration = u256_at(&bytes, head.permit_offset + 64);
    assert!(permit_expiration > 0, "PermitSingle.expiration must be set");

    // nonce word (should be 0 from fixture's zeroed Permit2 allowance)
    let permit_nonce = u256_at(&bytes, head.permit_offset + 96);
    assert_eq!(permit_nonce, 0, "PermitSingle.nonce should be 0 from fixture");

    // spender word
    let permit_spender = &bytes[head.permit_offset + 128 + 12..head.permit_offset + 128 + 32];
    assert_eq!(
        permit_spender,
        &decode_hex(&AGGREGATOR)[..],
        "PermitSingle.spender must be aggregator"
    );

    // sigDeadline word (should match expiration = deadline)
    let permit_sig_deadline = u256_at(&bytes, head.permit_offset + 160);
    assert_eq!(
        permit_sig_deadline, permit_expiration,
        "PermitSingle.sigDeadline must match expiration"
    );

    // signature length = 0 (unsigned; signature is spliced client-side)
    // sig_len is at offset 7*32 from start of Permit2Pull.
    let sig_len = u256_at(&bytes, head.permit_offset + 224);
    assert_eq!(sig_len, 0, "signature must be empty (unsigned)");
}

// ─── 2. Permit2 allowance with correct nonce propagation ────────────────────

/// Fixture: Permit2 allowance returns packed {amount=0, expiration=far_future, nonce=42}.
fn permit2_nonce_fixture() -> (String, std::thread::JoinHandle<()>) {
    // Nonce=42 packed into upper bits of the allowance word.
    let packed = packed_allowance_hex(0, u64::MAX, 42);
    fixture::spawn(move |method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            let sel4 = &data[..10];
            match sel4 {
                "0x5c975abb" => Ok(json!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                )),
                "0xdd62ed3e" => Ok(json!(
                    "0x00000000000000000000000000000000000000000000000000000000000f4240"
                )),
                "0x927da105" => Ok(json!(packed)),
                other => Err(json!(format!("unexpected eth_call selector {other}"))),
            }
        }
        other => Err(json!(format!("unexpected method {other}"))),
    })
}

#[tokio::test]
async fn build_tx_propagates_permit2_nonce_to_typed_data() {
    let (url, _server) = permit2_nonce_fixture();
    let router = test_app(Some(url)).await;
    let (status, body) = post(&router, "/api/v1/build_tx", valid_body()).await;
    assert_eq!(status, StatusCode::OK);

    let typed = body["data"]["typed_data"].as_object().expect("typed_data present");
    let message = typed["message"].as_object().unwrap();
    let details = message["details"].as_object().unwrap();

    // Nonce should be 42 (from the fixture).
    assert_eq!(
        details["nonce"].as_str().unwrap(),
        "42",
        "nonce must propagate from on-chain"
    );

    // Expiration should be set (non-zero, from deadline).
    let expiration: u64 = details["expiration"].as_str().unwrap().parse().unwrap();
    assert!(expiration > 0, "expiration must be set from deadline");

    // sigDeadline must match expiration.
    let sig_deadline: u64 = message["sigDeadline"].as_str().unwrap().parse().unwrap();
    assert_eq!(sig_deadline, expiration, "sigDeadline must match expiration");

    // Check that PermitSingle words in calldata are populated.
    let data = body["data"]["data"].as_str().unwrap();
    let bytes = decode_hex(data);
    let head = decode_head(&bytes);
    let permit_nonce = u256_at(&bytes, head.permit_offset + 96);
    assert_eq!(permit_nonce, 42, "PermitSingle.nonce in calldata must be 42");
}

// ─── 3. Permit2 allowance sufficient → no typed data ────────────────────────

#[tokio::test]
async fn build_tx_omits_typed_data_and_approvals_when_allowances_sufficient() {
    let (url, _server) = fully_approved_fixture();
    let router = test_app(Some(url)).await;
    let (status, body) = post(&router, "/api/v1/build_tx", valid_body()).await;
    assert_eq!(status, StatusCode::OK);
    let data = body["data"]["data"].as_str().unwrap();
    let bytes = decode_hex(data);
    let head = decode_head(&bytes);
    // When allowances are sufficient, typed_data is null and PermitSingle words zeroed.
    assert_eq!(body["data"]["typed_data"], Value::Null, "typedData must be omitted");
    let approvals = body["data"]["required_approvals"].as_array().unwrap();
    assert!(approvals.is_empty(), "no ERC-20 approval needed");
    // PermitSingle token word should be zeroed (no permit needed).
    let permit_token = &bytes[head.permit_offset..head.permit_offset + 32];
    assert_eq!(
        permit_token, &[0u8; 32],
        "PermitSingle.token must be zeroed when allowance sufficient"
    );
    let _ = head;
}

// ─── 4. Mutated path → ROUTE_INVALID without re-quoting ─────────────────────

#[tokio::test]
async fn build_tx_rejects_broken_continuity_without_requoting() {
    let (url, _server) = fixture::spawn(|method, params| match method {
        "eth_call" => {
            let data = params[0]["data"].as_str().unwrap();
            let sel4 = &data[..10];
            if sel4 == "0x927da105" {
                // Permit2 allowance: 3 zero words
                Ok(json!("0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"))
            } else {
                Ok(json!(
                    "0x0000000000000000000000000000000000000000000000000000000000000000"
                ))
            }
        }
        other => Err(json!(format!("unexpected {other}"))),
    });
    let router = test_app(Some(url)).await;

    let mut body = valid_body();
    body["sub_routes"][0]["steps"][0]["token_in"] = json!(EURC.to_ascii_lowercase());
    let (status, resp) = post(&router, "/api/v1/build_tx", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["code"], ApiErrorCode::RouteInvalid.as_str());

    let mut body = valid_body();
    body["sub_routes"][0]["amount_in"] = json!("999");
    let (status, resp) = post(&router, "/api/v1/build_tx", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["code"], ApiErrorCode::RouteInvalid.as_str());

    let mut body = valid_body();
    body["sub_routes"][0]["steps"][0]["pool_address"] = json!("0x00000000000000000000000000000000000000ff");
    let (status, resp) = post(&router, "/api/v1/build_tx", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["code"], ApiErrorCode::RouteInvalid.as_str());
}

// ─── 5. paused() = true → 503 PAUSED ────────────────────────────────────────

#[tokio::test]
async fn build_tx_returns_paused_when_aggregator_paused() {
    let (url, _server) = paused_fixture();
    let router = test_app(Some(url)).await;
    let (status, body) = post(&router, "/api/v1/build_tx", valid_body()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], ApiErrorCode::Paused.as_str());
}

// ─── 6. typed data requires PermitSingle when allowance insufficient ────────

#[tokio::test]
async fn build_tx_requires_typed_data_when_permit2_allowance_insufficient() {
    let (url, _server) = erc20_approved_fixture();
    let router = test_app(Some(url)).await;
    let (status, body) = post(&router, "/api/v1/build_tx", valid_body()).await;
    assert_eq!(status, StatusCode::OK);
    let typed = body["data"]["typed_data"].as_object().expect("typed_data present");
    assert_eq!(typed["types"]["PermitSingle"].is_array(), true, "must be PermitSingle");
    assert!(
        !typed["types"]
            .as_object()
            .unwrap()
            .contains_key("PermitWitnessTransferFrom"),
        "must NOT be SignatureTransfer/witness"
    );
    let domain = typed["domain"].as_object().unwrap();
    assert_eq!(
        domain["verifyingContract"].as_str().unwrap().to_ascii_lowercase(),
        PERMIT2.to_ascii_lowercase(),
        "verifyingContract must be Permit2"
    );
    let message = typed["message"].as_object().unwrap();
    assert_eq!(
        message["spender"].as_str().unwrap().to_ascii_lowercase(),
        AGGREGATOR,
        "spender must be the aggregator"
    );
    let details = message["details"].as_object().unwrap();
    assert_eq!(
        details["token"].as_str().unwrap().to_ascii_lowercase(),
        USDC_ERC20.to_ascii_lowercase(),
        "details.token must be the input token"
    );
    let approvals = body["data"]["required_approvals"].as_array().unwrap();
    assert!(approvals.is_empty(), "ERC-20 allowance already sufficient");
}

// ─── 7. Empty aggregator config → NOT_READY ─────────────────────────────────

#[tokio::test]
async fn build_tx_not_ready_when_aggregator_unconfigured() {
    let (url, _server) = permit_needed_fixture();
    let mut config = app_config();
    config.chakra_aggregator = String::new();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&chakra_snapshot()).await.unwrap();
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    seed_pools(&pool_store).await;
    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&chakra_snapshot(), MBTC).await;
    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn market_snapshot::store::SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn market_snapshot::pool_state_store::PoolStateStore>),
        Some(snapshot_store),
        Some(pool_store),
        Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        MBTC.to_string(),
        None,
    )
    .await;
    let router = build_router(state, RateLimitState::from_env());
    let (status, body) = post(&router, "/api/v1/build_tx", valid_body()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], ApiErrorCode::NotReady.as_str());
}

// ─── 8. Two factories: only exact factory address matches ────────────────────

/// Snapshot with two XYK pools from different factories.
fn two_factory_snapshot() -> MarketSnapshot {
    MarketSnapshot::from_sources(
        "chakra-build-2fact",
        1_700_000_000_000,
        "arc-testnet",
        vec![SourceSnapshot {
            source: "chakra-xyk".to_string(),
            pairs: vec![
                TradingPairSnapshot {
                    token_a: USDC_ERC20.to_string(),
                    token_b: EURC.to_string(),
                    pool_address: XYK_POOL_UE.to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: "0x0000000000000000000000000000000000000001".to_string(),
                },
                TradingPairSnapshot {
                    token_a: USDC_ERC20.to_string(),
                    token_b: EURC.to_string(),
                    pool_address: "0x0000000000000000000000000000000000000003".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: "0x0000000000000000000000000000000000000002".to_string(),
                },
            ],
        }],
    )
}

#[tokio::test]
async fn build_tx_rejects_pool_from_non_allowlisted_factory() {
    use market_snapshot::pool_state_store::FactoryRecord;

    let (url, _server) = permit_needed_fixture();
    let config = app_config();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&two_factory_snapshot()).await.unwrap();
    let pool_store = Arc::new(MemoryPoolStateStore::new());

    // Only allowlist the second factory (0x...0002).
    pool_store
        .set_factories(&[FactoryRecord::new(
            "0x0000000000000000000000000000000000000002",
            "xyk",
            "chakra-xyk",
        )])
        .await
        .unwrap();

    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&two_factory_snapshot(), MBTC).await;

    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn market_snapshot::store::SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn market_snapshot::pool_state_store::PoolStateStore>),
        Some(snapshot_store),
        Some(pool_store),
        Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        MBTC.to_string(),
        None,
    )
    .await;
    let router = build_router(state, RateLimitState::from_env());

    // Use XYK_POOL_UE which has factory 0x...0001 (not allowlisted).
    let body = json!({
        "user": USER,
        "token_in": USDC_ERC20.to_ascii_lowercase(),
        "token_out": EURC.to_ascii_lowercase(),
        "amount_in": "1000000",
        "min_amount_out": "990000",
        "sub_routes": [{
            "amount_in": "1000000",
            "steps": [{
                "dex_type": "xyk",
                "pool_address": XYK_POOL_UE,
                "token_in": USDC_ERC20.to_ascii_lowercase(),
                "token_out": EURC.to_ascii_lowercase()
            }]
        }]
    });
    let (status, resp) = post(&router, "/api/v1/build_tx", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["code"], ApiErrorCode::RouteInvalid.as_str());
}

// ─── 9. CLMM fee validation ────────────────────────────────────────────────

const CLMM_POOL: &str = "0x0000000000000000000000000000000000000010";
const CLMM_FACTORY: &str = "0x00000000000000000000000000000000000000f1";

/// Snapshot with a CLMM pool at 30 bps fee.
fn clmm_snapshot() -> MarketSnapshot {
    MarketSnapshot::from_sources(
        "chakra-build-clmm",
        1_700_000_000_000,
        "arc-testnet",
        vec![SourceSnapshot {
            source: "chakra-clmm".to_string(),
            pairs: vec![],
        }],
    )
    .with_clmm_pool_refs(vec![market_snapshot::ClmmPoolRefSnapshot {
        source: "chakra-clmm".to_string(),
        pool_address: CLMM_POOL.to_string(),
        token0: USDC_ERC20.to_string(),
        token1: EURC.to_string(),
        fee_bps: 30,
        tick_spacing: 10,
        factory: CLMM_FACTORY.to_string(),
    }])
}

fn clmm_body_with_fee(fee_bps: u32) -> Value {
    json!({
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
                "fee_bps": fee_bps
            }]
        }]
    })
}

#[tokio::test]
async fn build_tx_rejects_wrong_fee_for_clmm_pool() {
    use market_snapshot::pool_state_store::FactoryRecord;
    let (url, _server) = permit_needed_fixture();
    let mut config = app_config();
    config.chakra_aggregator = AGGREGATOR.to_string();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&clmm_snapshot()).await.unwrap();
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    pool_store
        .set_factories(&[FactoryRecord::new(CLMM_FACTORY, "clmm", "chakra-clmm")])
        .await
        .unwrap();

    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&clmm_snapshot(), MBTC).await;

    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn market_snapshot::store::SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn market_snapshot::pool_state_store::PoolStateStore>),
        Some(snapshot_store),
        Some(pool_store),
        Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        MBTC.to_string(),
        None,
    )
    .await;
    let router = build_router(state, RateLimitState::from_env());

    // Submit fee_bps = 5 but snapshot says 30 → should reject.
    let body = clmm_body_with_fee(5);
    let (status, resp) = post(&router, "/api/v1/build_tx", body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["error"]["code"], ApiErrorCode::RouteInvalid.as_str());
    let msg = resp["error"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("fee") || msg.contains("Fee"),
        "error should mention fee, got: {msg}"
    );
}

#[tokio::test]
async fn build_tx_accepts_correct_fee_for_clmm_pool() {
    use market_snapshot::pool_state_store::FactoryRecord;
    let (url, _server) = permit_needed_fixture();
    let mut config = app_config();
    config.chakra_aggregator = AGGREGATOR.to_string();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&clmm_snapshot()).await.unwrap();
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    pool_store
        .set_factories(&[FactoryRecord::new(CLMM_FACTORY, "clmm", "chakra-clmm")])
        .await
        .unwrap();

    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&clmm_snapshot(), MBTC).await;

    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn market_snapshot::store::SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn market_snapshot::pool_state_store::PoolStateStore>),
        Some(snapshot_store),
        Some(pool_store),
        Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        MBTC.to_string(),
        None,
    )
    .await;
    let router = build_router(state, RateLimitState::from_env());

    // Submit fee_bps = 30 matching snapshot → should pass.
    let body = clmm_body_with_fee(30);
    let (status, _body) = post(&router, "/api/v1/build_tx", body).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn build_tx_encodes_step_fee_not_hardcoded_30() {
    use market_snapshot::pool_state_store::FactoryRecord;
    let (url, _server) = permit_needed_fixture();
    let mut config = app_config();
    config.chakra_aggregator = AGGREGATOR.to_string();
    let snapshot_store = Arc::new(MemorySnapshotStore::new());
    snapshot_store.publish_snapshot(&clmm_snapshot()).await.unwrap();
    let pool_store = Arc::new(MemoryPoolStateStore::new());
    pool_store
        .set_factories(&[FactoryRecord::new(CLMM_FACTORY, "clmm", "chakra-clmm")])
        .await
        .unwrap();

    let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
    engine.update_from_chakra_snapshot(&clmm_snapshot(), MBTC).await;

    let state = AppState::from_backends(
        config,
        Some(snapshot_store.clone() as Arc<dyn market_snapshot::store::SnapshotStore>),
        Some(pool_store.clone() as Arc<dyn market_snapshot::pool_state_store::PoolStateStore>),
        Some(snapshot_store),
        Some(pool_store),
        Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(&url).unwrap())),
        MBTC.to_string(),
        None,
    )
    .await;
    let router = build_router(state, RateLimitState::from_env());

    // Encode with fee_bps = 30 (matching snapshot).
    let body = clmm_body_with_fee(30);
    let (status, body_resp) = post(&router, "/api/v1/build_tx", body).await;
    assert_eq!(status, StatusCode::OK);
    let data_hex = body_resp["data"]["data"].as_str().unwrap();
    let bytes = decode_hex(data_hex);
    let head = decode_head(&bytes);

    // Routes block: [count][offset_to_SubRoute0][SubRoute0_data...]
    // The element offset is relative to the start of the head (after count word).
    let routes_base = head.routes_offset;
    let _count = u256_at(&bytes, routes_base) as usize;
    let sub0_offset = u256_at(&bytes, routes_base + 32) as usize;
    let sub_base = routes_base + 32 + sub0_offset;
    // SubRoute: amountIn (word 0), hopsOffset (word 1), Hop[] data...
    let hops_offset = u256_at(&bytes, sub_base + 32) as usize;
    let hops_base = sub_base + hops_offset;
    // Hop count is the first word of the dynamic Hop[].
    let hop_count = u256_at(&bytes, hops_base) as usize;
    assert_eq!(hop_count, 1, "should have exactly 1 hop");

    // Hop layout: pool(32) + dexType(32) + tokenIn(32) + tokenOut(32) + fee(32)
    let hop_data = hops_base + 32; // skip the length word
    let fee_encoded = u256_at(&bytes, hop_data + 4 * 32);
    assert_eq!(fee_encoded, 30, "fee must be 30 bps from the step, not hardcoded");
}
