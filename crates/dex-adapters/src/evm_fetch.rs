//! EVM state hydrators for Arc venues (T3.3): fetch **touched** pools only via
//! `eth_call`, never a full-market sweep on the hot path.
//!
//! - xy=k: `getReserves()` → [`market_snapshot::pool_state_store::XykPoolStateValue`]
//! - stable: ERC-20 `balanceOf` both tokens → `StablePoolStateValue`
//! - CLMM: `slot0()` + `liquidity()` merged over the existing snapshot (tick
//!   coverage untouched); Redis publish still gated by
//!   `should_publish_clmm_to_redis`.
//!
//! Factory discovery (`getPair` / `getPool`) backs the ~600 s topology rebuild.

/// Amplification coefficient of the Chakra stableswap factory (`A = 100`).
pub const CHAKRA_STABLE_A: u128 = 100;

use {
    crate::{
        evm_logs::{function_selector_hex, normalize_evm_address, word_to_address},
        evm_rpc::{decode_hex, word_to_i32, word_to_u128, word_to_u256_limbs, EvmRpcClient},
    },
    anyhow::{bail, Context, Result},
    market_snapshot::{
        pool_state_store::{StablePoolStateValue, XykPoolStateValue},
        ClmmPoolRefSnapshot, ClmmPoolSnapshot, TradingPairSnapshot,
    },
};

// ─── Calldata builders ──────────────────────────────────────────────────────

/// 32-byte ABI word for an address argument.
pub fn encode_address_arg(address: &str) -> Result<String> {
    let hex = address.strip_prefix("0x").unwrap_or(address);
    if hex.len() != 40 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("not an EVM address: {address}");
    }
    Ok(format!("0x{hex:0>64}"))
}

/// 32-byte ABI word for a `u128`/`uint256` argument (right-aligned).
pub fn encode_uint_arg(value: u128) -> String {
    format!("0x{value:0>64x}")
}

/// Append 32-byte words to a selector.
pub fn calldata(selector: &str, words: &[String]) -> String {
    let mut data = selector.to_string();
    for word in words {
        data.push_str(word.trim_start_matches("0x"));
    }
    data
}

/// Split a `0x`-prefixed response into 32-byte words (big-endian chunks).
pub fn split_words(response: &str) -> Result<Vec<[u8; 32]>> {
    let bytes = decode_hex(response).context("decode eth_call response")?;
    if bytes.is_empty() || bytes.len() % 32 != 0 {
        bail!("eth_call response is not whole 32-byte words: {} bytes", bytes.len());
    }
    Ok(bytes.chunks_exact(32).map(|chunk| chunk.try_into().unwrap()).collect())
}

fn word_hex(word: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(word))
}

fn word_to_u128_bytes(word: &[u8; 32]) -> Result<u128> {
    word_to_u128(&word_hex(word))
}

fn word_to_i32_bytes(word: &[u8; 32]) -> Result<i32> {
    word_to_i32(&word_hex(word))
}

// ─── Selectors (keccak256(function signature)) ──────────────────────────────

pub fn get_reserves_selector() -> String {
    function_selector_hex("getReserves()")
}
pub fn balance_of_selector() -> String {
    function_selector_hex("balanceOf(address)")
}
pub fn slot0_selector() -> String {
    function_selector_hex("slot0()")
}
pub fn liquidity_selector() -> String {
    function_selector_hex("liquidity()")
}
pub fn get_pair_selector() -> String {
    function_selector_hex("getPair(address,address)")
}
pub fn get_pool_v3_selector() -> String {
    function_selector_hex("getPool(address,address,uint24)")
}
pub fn get_pool_stable_selector() -> String {
    function_selector_hex("getPool(address,address)")
}
pub fn get_amplification_selector() -> String {
    function_selector_hex("getAmplificationParameter()")
}

// ─── Hydrators ──────────────────────────────────────────────────────────────

/// `getReserves()` → xy=k pool state (reserve0/reserve1, timestamp ignored).
pub async fn fetch_xyk_state(
    client: &EvmRpcClient,
    source: &str,
    pair: &TradingPairSnapshot,
) -> Result<XykPoolStateValue> {
    let response = client
        .eth_call(&pair.pool_address, &calldata(&get_reserves_selector(), &[]))
        .await?;
    let words = split_words(&response)?;
    if words.len() < 2 {
        bail!("getReserves returned {} words", words.len());
    }
    let (reserve0, reserve1) = (word_to_u128_bytes(&words[0])?, word_to_u128_bytes(&words[1])?);
    let mut value = XykPoolStateValue::new(
        source,
        &pair.pool_address,
        &pair.token_a,
        &pair.token_b,
        pair.fee_bps,
        reserve0,
        reserve1,
    );
    value.factory = pair.factory.clone();
    Ok(value)
}

/// ERC-20 `balanceOf(pool)` on both tokens → stable pool state.
pub async fn fetch_stable_state(
    client: &EvmRpcClient,
    source: &str,
    pair: &TradingPairSnapshot,
    a: u128,
) -> Result<StablePoolStateValue> {
    let balance_of = calldata(&balance_of_selector(), &[encode_address_arg(&pair.pool_address)?]);
    let balance_a_word = client.eth_call(&pair.token_a, &balance_of).await?;
    let balance_b_word = client.eth_call(&pair.token_b, &balance_of).await?;
    let balance_a = word_to_u128(&balance_a_word).context("decode balanceOf token_a")?;
    let balance_b = word_to_u128(&balance_b_word).context("decode balanceOf token_b")?;
    let mut value = StablePoolStateValue::new(
        source,
        &pair.pool_address,
        &pair.token_a,
        &pair.token_b,
        balance_a,
        balance_b,
        a,
        pair.fee_bps,
    );
    value.factory = pair.factory.clone();
    Ok(value)
}

/// XyloNet pool state (T-XYLO): `getReserves()` (stored reserves — the venue
/// `calculateSwap` uses `reserve0/reserve1`, not balances) + the **hydrated
/// on-chain amplification** (`getAmplificationParameter()` = raw amp, e.g.
/// 20000 → A=200 after `A_PRECISION=100`) + 4 bps fee on output. Reuses the
/// stableswap value shape; the QuoteEngine dispatches `xylo-stable` sources
/// to `xylo_quote`. 2026-08-29: A is read from the pool, not hardcoded.
pub async fn fetch_xylo_state(
    client: &EvmRpcClient,
    source: &str,
    pair: &TradingPairSnapshot,
) -> Result<StablePoolStateValue> {
    let response = client
        .eth_call(&pair.pool_address, &calldata(&get_reserves_selector(), &[]))
        .await?;
    let words = split_words(&response)?;
    if words.len() < 2 {
        bail!("xylo getReserves returned {} words", words.len());
    }
    let (reserve0, reserve1) = (word_to_u128_bytes(&words[0])?, word_to_u128_bytes(&words[1])?);
    // Hydrate the amplification from the pool: `getAmplificationParameter()`
    // returns the raw amp (20000); divide by A_PRECISION (100) → A=200.
    let amp_raw = client
        .eth_call(&pair.pool_address, &calldata(&get_amplification_selector(), &[]))
        .await
        .ok()
        .and_then(|r| split_words(&r).ok())
        .and_then(|w| w.first().cloned())
        .and_then(|w| word_to_u128_bytes(&w).ok())
        .unwrap_or(0);
    let a = if amp_raw > 0 { amp_raw / 100 } else { 0 };
    let mut value = StablePoolStateValue::new(
        source,
        &pair.pool_address,
        &pair.token_a,
        &pair.token_b,
        reserve0,
        reserve1,
        a,
        pair.fee_bps,
    );
    value.factory = pair.factory.clone();
    Ok(value)
}

/// `slot0()` + `liquidity()` merged over the existing snapshot. Tick/bitmap
/// coverage is carried through untouched — if the pool was never fully loaded
/// (`coverage` is `None` or incomplete), the caller must skip the Redis write.
pub async fn fetch_clmm_state(
    client: &EvmRpcClient,
    source: &str,
    pool_ref: &ClmmPoolRefSnapshot,
    existing: Option<&ClmmPoolSnapshot>,
) -> Result<ClmmPoolSnapshot> {
    let slot0 = client
        .eth_call(&pool_ref.pool_address, &calldata(&slot0_selector(), &[]))
        .await?;
    let words = split_words(&slot0)?;
    if words.is_empty() {
        bail!("slot0 returned no words");
    }
    let sqrt_price_x96 = word_to_u256_limbs(&word_hex(&words[0]))?;
    let tick = word_to_i32_bytes(&words[1]).unwrap_or(0);
    let liquidity_word = client
        .eth_call(&pool_ref.pool_address, &calldata(&liquidity_selector(), &[]))
        .await?;
    let liquidity = word_to_u128(&liquidity_word).context("decode liquidity")?;

    let existing = existing.cloned();
    Ok(ClmmPoolSnapshot {
        source: source.to_string(),
        pool_address: pool_ref.pool_address.clone(),
        token0: pool_ref.token0.clone(),
        token1: pool_ref.token1.clone(),
        fee_bps: pool_ref.fee_bps,
        tick_spacing: pool_ref.tick_spacing,
        sqrt_price_x96,
        tick,
        liquidity,
        factory: existing.as_ref().map(|p| p.factory.clone()).unwrap_or_default(),
        ticks: existing.as_ref().map(|p| p.ticks.clone()).unwrap_or_default(),
        chunk_bitmaps: existing.as_ref().map(|p| p.chunk_bitmaps.clone()).unwrap_or_default(),
        word_bitmaps: existing.as_ref().map(|p| p.word_bitmaps.clone()).unwrap_or_default(),
        coverage: existing.as_ref().and_then(|p| p.coverage.clone()),
    })
}

// ─── Factory discovery (topology rebuild, ~600 s) ───────────────────────────

/// Asks a V2-style factory for the pair; non-zero address → `Some`.
pub async fn factory_has_xyk_pair(
    client: &EvmRpcClient,
    factory: &str,
    token_a: &str,
    token_b: &str,
) -> Result<Option<String>> {
    let data = calldata(
        &get_pair_selector(),
        &[encode_address_arg(token_a)?, encode_address_arg(token_b)?],
    );
    let response = client.eth_call(factory, &data).await?;
    let words = split_words(&response)?;
    let Some(word) = words.first() else {
        return Ok(None);
    };
    let address = word_to_address(&word_hex(word)).context("decode getPair result")?;
    Ok((word != &[0u8; 32]).then(|| normalize_evm_address(&address)))
}

/// Asks a V3-style factory for `getPool(token0, token1, fee)`.
pub async fn factory_has_clmm_pool(
    client: &EvmRpcClient,
    factory: &str,
    token_a: &str,
    token_b: &str,
    fee: u32,
) -> Result<Option<String>> {
    let data = calldata(
        &get_pool_v3_selector(),
        &[
            encode_address_arg(token_a)?,
            encode_address_arg(token_b)?,
            encode_uint_arg(fee as u128),
        ],
    );
    let response = client.eth_call(factory, &data).await?;
    let words = split_words(&response)?;
    let Some(word) = words.first() else {
        return Ok(None);
    };
    let address = word_to_address(&word_hex(word)).context("decode getPool result")?;
    Ok((word != &[0u8; 32]).then(|| normalize_evm_address(&address)))
}

/// Asks a Chakra stable factory (`getPool(address,address)` mapping getter).
pub async fn factory_has_stable_pool(
    client: &EvmRpcClient,
    factory: &str,
    token_a: &str,
    token_b: &str,
) -> Result<Option<String>> {
    let data = calldata(
        &get_pool_stable_selector(),
        &[encode_address_arg(token_a)?, encode_address_arg(token_b)?],
    );
    let response = client.eth_call(factory, &data).await?;
    let words = split_words(&response)?;
    let Some(word) = words.first() else {
        return Ok(None);
    };
    let address = word_to_address(&word_hex(word)).context("decode getPool result")?;
    Ok((word != &[0u8; 32]).then(|| normalize_evm_address(&address)))
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::evm_rpc::fixture,
        market_snapshot::{pool_state_store::should_publish_clmm_to_redis, ClmmCoverageSnapshot},
        serde_json::json,
    };

    const USDC: &str = "0x3600000000000000000000000000000000000000";
    const EURC: &str = "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a";
    const MBTC: &str = "0x1111111111111111111111111111111111111111";
    const POOL: &str = "0x2222222222222222222222222222222222222222";

    fn word(value: u128) -> String {
        format!("0x{value:0>64x}")
    }

    /// Multi-word hex payload without per-word `0x` prefixes (ABI concatenation).
    fn words_hex(values: &[u128]) -> String {
        let mut data = "0x".to_string();
        for value in values {
            data.push_str(&format!("{value:0>64x}"));
        }
        data
    }

    /// `sqrtPriceX96 = 10 * 2^96`.
    fn sqrt_price_hex() -> String {
        format!("{:0>64x}", 10u128 << 96)
    }

    fn reserves_response(r0: u128, r1: u128) -> String {
        // 3 words: reserve0, reserve1, blockTimestampLast.
        words_hex(&[r0, r1, 0])
    }

    fn xyk_pair() -> TradingPairSnapshot {
        TradingPairSnapshot {
            token_a: USDC.to_string(),
            token_b: EURC.to_string(),
            pool_address: POOL.to_string(),
            fee_bps: 30,
            dex_type: "xyk".to_string(),
            factory: "0xFAC".to_string(),
        }
    }

    #[tokio::test]
    async fn fetch_xyk_hydrates_from_get_reserves() {
        let (url, _server) = fixture::spawn(|method, params| {
            assert_eq!(method, "eth_call");
            assert_eq!(params[0]["to"], POOL);
            assert_eq!(params[0]["data"], "0x0902f1ac"); // getReserves()
            Ok(json!(reserves_response(50_000_000_000, 1_000_000_000)))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let state = fetch_xyk_state(&client, "chakra-xyk", &xyk_pair()).await.unwrap();
        assert_eq!(state.reserve_a, 50_000_000_000);
        assert_eq!(state.reserve_b, 1_000_000_000);
        assert_eq!(state.token_a, USDC);
        assert_eq!(state.token_b, EURC);
        assert_eq!(state.fee_bps, 30);
        assert_eq!(state.factory, "0xFAC");
    }

    #[tokio::test]
    async fn fetch_stable_reads_balance_of_both_tokens() {
        let (url, _server) = fixture::spawn(|method, params| {
            assert_eq!(method, "eth_call");
            let to = params[0]["to"].as_str().unwrap();
            match to {
                USDC => Ok(json!(word(200_000_000_000))),
                EURC => Ok(json!(word(199_999_000_000))),
                other => panic!("unexpected to: {other}"),
            }
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let pair = TradingPairSnapshot {
            token_a: USDC.to_string(),
            token_b: EURC.to_string(),
            pool_address: POOL.to_string(),
            fee_bps: 4,
            dex_type: "stable".to_string(),
            factory: "0xFAC".to_string(),
        };
        let state = fetch_stable_state(&client, "chakra-stable", &pair, 100).await.unwrap();
        assert_eq!(state.balance_a, 200_000_000_000);
        assert_eq!(state.balance_b, 199_999_000_000);
        assert_eq!(state.a, 100);
        assert_eq!(state.fee_bps, 4);
    }

    #[tokio::test]
    async fn fetch_clmm_merges_slot0_liquidity_and_preserves_coverage() {
        // slot0: sqrtPriceX96 = 10 * 2^96, tick = -60, rest zero words.
        let tick_value = (-60i32) as u32 as u128;
        let slot0_response = words_hex(&[
            // sqrtPriceX96 as a 160-bit value in the low bits of the word.
            u128::from_str_radix(&sqrt_price_hex()[..], 16).unwrap(),
            tick_value,
            0,
            0,
            0,
            0,
            0,
        ]);
        let (url, _server) = fixture::spawn(move |method, params| {
            assert_eq!(method, "eth_call");
            let data = params[0]["data"].as_str().unwrap();
            match data {
                "0x3850c7bd" => Ok(json!(slot0_response.clone())),  // slot0()
                "0x1a686502" => Ok(json!(word(1_000_000_000_000))), // liquidity()
                other => panic!("unexpected data: {other}"),
            }
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let pool_ref = ClmmPoolRefSnapshot {
            source: "chakra-clmm".to_string(),
            pool_address: POOL.to_string(),
            token0: USDC.to_string(),
            token1: MBTC.to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            factory: "0xFAC".to_string(),
        };
        let existing = ClmmPoolSnapshot {
            source: "chakra-clmm".to_string(),
            pool_address: POOL.to_string(),
            token0: USDC.to_string(),
            token1: MBTC.to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [0; 4],
            tick: 0,
            liquidity: 0,
            factory: "0xFAC".to_string(),
            ticks: vec![],
            chunk_bitmaps: vec![],
            word_bitmaps: vec![],
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(-887220),
                max_loaded_tick: Some(887220),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        };

        let snapshot = fetch_clmm_state(&client, "chakra-clmm", &pool_ref, Some(&existing))
            .await
            .unwrap();
        assert_eq!(snapshot.sqrt_price_x96, [0, 0xa00000000, 0, 0]);
        assert_eq!(snapshot.tick, -60);
        assert_eq!(snapshot.liquidity, 1_000_000_000_000);
        assert_eq!(snapshot.coverage, existing.coverage);
        assert!(should_publish_clmm_to_redis(&snapshot));
    }

    #[tokio::test]
    async fn fetch_clmm_without_coverage_is_not_publishable() {
        let slot0_response = words_hex(&[
            u128::from_str_radix(&sqrt_price_hex()[..], 16).unwrap(),
            0,
            0,
            0,
            0,
            0,
            0,
        ]);
        let (url, _server) = fixture::spawn(move |method, params| {
            let data = params[0]["data"].as_str().unwrap();
            match data {
                "0x3850c7bd" => Ok(json!(slot0_response.clone())),
                "0x1a686502" => Ok(json!(word(5))),
                other => panic!("unexpected data: {other}"),
            }
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let pool_ref = ClmmPoolRefSnapshot {
            source: "chakra-clmm".to_string(),
            pool_address: POOL.to_string(),
            token0: USDC.to_string(),
            token1: MBTC.to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            factory: "0xFAC".to_string(),
        };
        let snapshot = fetch_clmm_state(&client, "chakra-clmm", &pool_ref, None).await.unwrap();
        assert_eq!(snapshot.coverage, None);
        assert!(!should_publish_clmm_to_redis(&snapshot));
    }

    #[tokio::test]
    async fn discovery_get_pair_returns_zero_for_missing_pool() {
        let (url, _server) = fixture::spawn(|method, params| {
            assert_eq!(method, "eth_call");
            assert_eq!(params[0]["to"], "0x00000000000000000000000000000000000000fa");
            let data = params[0]["data"].as_str().unwrap();
            // 0x + selector(4) + two address words.
            assert_eq!(data.len(), 2 + 8 + 128);
            Ok(json!(word(0)))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let pair = factory_has_xyk_pair(&client, "0x00000000000000000000000000000000000000fa", USDC, EURC)
            .await
            .unwrap();
        assert_eq!(pair, None);
    }

    #[tokio::test]
    async fn discovery_get_pair_returns_pool_address() {
        let (url, _server) = fixture::spawn(|_method, _params| {
            Ok(json!(format!("0x{:0>64}", "2222222222222222222222222222222222222222")))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let pool = factory_has_xyk_pair(&client, "0xfac", USDC, EURC).await.unwrap();
        assert_eq!(pool.as_deref(), Some(POOL));
    }

    #[tokio::test]
    async fn discovery_get_pool_v3_includes_fee_arg() {
        let (url, _server) = fixture::spawn(|_method, params| {
            let data = params[0]["data"].as_str().unwrap();
            // 0x + selector(4) + 3 words: token_a, token_b, fee(3000).
            assert_eq!(data.len(), 2 + 8 + 64 * 3);
            let bytes = crate::evm_rpc::decode_hex(data).unwrap();
            assert_eq!(bytes.len(), 4 + 96);
            // fee is word 3's low 32 bits (bytes 96..100).
            let fee = u32::from_be_bytes(bytes[96..100].try_into().unwrap());
            assert_eq!(fee, 3000);
            Ok(json!(word(0)))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let pool = factory_has_clmm_pool(&client, "0xfac", USDC, EURC, 3000).await.unwrap();
        assert_eq!(pool, None);
    }

    #[tokio::test]
    async fn calldata_and_encode_helpers() {
        assert_eq!(
            encode_address_arg(USDC).unwrap(),
            format!("0x{:0>64}", "3600000000000000000000000000000000000000")
        );
        assert!(encode_address_arg("not-an-address").is_err());
        let data = calldata(
            &get_pair_selector(),
            &[encode_address_arg(USDC).unwrap(), encode_address_arg(EURC).unwrap()],
        );
        assert_eq!(data.len(), 2 + 8 + 128);
        assert_eq!(get_reserves_selector(), "0x0902f1ac");
        assert_eq!(balance_of_selector(), "0x70a08231");
        assert_eq!(slot0_selector(), "0x3850c7bd");
        assert_eq!(liquidity_selector(), "0x1a686502");
        let words = split_words(&reserves_response(1, 2)).unwrap();
        assert_eq!(words.len(), 3);
        assert_eq!(word_to_u128_bytes(&words[0]).unwrap(), 1);
        // Local math cross-check: hydrated reserves feed xyk_quote.
        assert_eq!(crate::evm_quote_math::xyk_quote(10_000, 10_000, 1_000), 906);
    }
}
