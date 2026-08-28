//! T4.4 `/build_tx` splitSwap calldata encoder + Permit2 typed data.
//!
//! The encoder is **not** a re-quoter: it validates continuity, amount sum,
//! and snapshot + `chakra:factories` membership of every hop, then encodes
//! `splitSwap(...)` calldata. RPC (fixture/live): `paused()`, ERC-20
//! `allowance(user, Permit2)`, Permit2 `allowance(user, tokenIn, aggregator)`.
//! `value` is always `"0"`; default `deadline` = now + 120 s.

use {
    crate::{
        abi,
        envelope::ApiErrorCode,
        handlers::{BuildTxRequest, BuildTxStep, BuildTxSubRoute},
        state::AppState,
    },
    anyhow::{anyhow, bail, Result},
    dex_adapters::evm_rpc::EvmRpcClient,
    market_snapshot::{
        pool_state_store::{FactoryRecord, PoolStateStore},
        store::SnapshotStore,
        MarketSnapshot,
    },
    serde_json::{json, Value},
    std::time::{SystemTime, UNIX_EPOCH},
};

pub const SPLIT_SWAP_SIGNATURE: &str =
    "splitSwap(address,address,uint256,uint256,uint256,(uint256,(address,uint8,address,address,uint24)[])[],(((address,uint160,uint48,uint48),address,uint256),bytes))";
pub const PERMIT2_ADDRESS: &str = "0x000000000022D473030F116dDEE9F6B43aC78BA3";
pub const DEFAULT_DEADLINE_SECS: u64 = 120;

/// `paused()` selector.
const PAUSED_SELECTOR: &str = "0x5c975abb";
/// ERC-20 `allowance(address,address)` selector.
const ERC20_ALLOWANCE_SELECTOR: &str = "0xdd62ed3e";
/// Permit2 `allowance(address,address,address)` selector.
const PERMIT2_ALLOWANCE_SELECTOR: &str = "0x927da105";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DexType {
    Xyk,
    Stable,
    Clmm,
    /// XyloNet stableswap (T-XYLO) — appended, never inserted (on-chain enum
    /// values must stay stable).
    Xylo,
}

impl DexType {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "xyk" => Some(Self::Xyk),
            "stable" => Some(Self::Stable),
            "clmm" => Some(Self::Clmm),
            "xylo" => Some(Self::Xylo),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Xyk => 0,
            Self::Stable => 1,
            Self::Clmm => 2,
            Self::Xylo => 3,
        }
    }
}

/// One encoded hop: `(pool, dexType, tokenIn, tokenOut, fee)`.
fn encode_hop(step: &BuildTxStep, dex_type: DexType, fee_bps: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    out.extend_from_slice(&abi::address_word(&step.pool_address)?);
    out.extend_from_slice(&abi::uint_word(dex_type.as_u8() as u128));
    out.extend_from_slice(&abi::address_word(&step.token_in)?);
    out.extend_from_slice(&abi::address_word(&step.token_out)?);
    out.extend_from_slice(&abi::uint24_word(fee_bps));
    Ok(out)
}

/// One `SubRoute { uint256 amountIn; Hop[] hops; }`.
fn encode_sub_route(sub: &BuildTxSubRoute, snapshot: &MarketSnapshot) -> Result<Vec<u8>> {
    let mut hops = Vec::new();
    hops.extend_from_slice(&abi::uint_word(sub.steps.len() as u128));
    for step in &sub.steps {
        let dex_type = DexType::parse(&step.dex_type).ok_or_else(|| anyhow!("unknown dex_type: {}", step.dex_type))?;
        // T4.6: Fee resolution order — the per-hop fee from the quote when
        // provided, else the snapshot pool fee (so an omitted fee still
        // encodes the exact CLMM tier, e.g. 5 bps), else the venue default.
        let fee_bps = match step.fee_bps {
            Some(fee) => fee,
            None => snapshot_pool_fee(snapshot, &step.pool_address, dex_type).unwrap_or_else(|| step_fee_bps(dex_type)),
        };
        // Hop is entirely static, so Hop[] stores tuples inline without an
        // element-offset table.
        hops.extend_from_slice(&encode_hop(step, dex_type, fee_bps)?);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&abi::uint_word(sub.amount_in.parse::<u128>()?));
    out.extend_from_slice(&abi::uint_word(64)); // offset to hops (amountIn + offset = 2 words)
    out.extend_from_slice(&hops);
    Ok(out)
}

/// Venue fee in bps: xy=k 30, stable 4, clmm 30, xylo 4 (frozen).
fn step_fee_bps(dex_type: DexType) -> u32 {
    match dex_type {
        DexType::Xyk | DexType::Clmm => 30,
        DexType::Stable | DexType::Xylo => 4,
    }
}

/// PermitSingle fields that go into the Permit2Pull struct.
#[derive(Debug, Clone, Copy)]
pub struct PermitSingleFields {
    pub token: [u8; 20],
    pub amount: u128,
    pub expiration: u64,
    pub nonce: u64,
    pub spender: [u8; 20],
    pub sig_deadline: u64,
}

impl Default for PermitSingleFields {
    fn default() -> Self {
        Self {
            token: [0u8; 20],
            amount: 0,
            expiration: 0,
            nonce: 0,
            spender: [0u8; 20],
            sig_deadline: 0,
        }
    }
}

/// `Permit2Pull { PermitSingle permitSingle; bytes signature; }`.
/// When `permit` is provided, the 6 PermitSingle words are populated;
/// otherwise all zeroed (empty-signature path skips `permit()` on-chain).
fn encode_permit2_pull(signature: &[u8], permit: Option<&PermitSingleFields>) -> Vec<u8> {
    let mut out = Vec::new();
    // PermitSingle struct: (address token, uint160 amount, uint48 expiration,
    // uint48 nonce, address spender, uint256 sigDeadline) — 6 words.
    match permit {
        Some(p) => {
            // token (address → left-aligned 32 bytes)
            let mut token_word = [0u8; 32];
            token_word[12..32].copy_from_slice(&p.token);
            out.extend_from_slice(&token_word);
            // amount (uint160 → right-aligned)
            out.extend_from_slice(&abi::uint_word(p.amount));
            // expiration (uint48 → right-aligned)
            out.extend_from_slice(&abi::uint_word(p.expiration as u128));
            // nonce (uint48 → right-aligned)
            out.extend_from_slice(&abi::uint_word(p.nonce as u128));
            // spender (address → left-aligned)
            let mut spender_word = [0u8; 32];
            spender_word[12..32].copy_from_slice(&p.spender);
            out.extend_from_slice(&spender_word);
            // sigDeadline (uint256 → right-aligned)
            out.extend_from_slice(&abi::uint_word(p.sig_deadline as u128));
        }
        None => {
            // All zeroed for empty-signature path (permit() skipped on-chain).
            for _ in 0..6 {
                out.extend_from_slice(&[0u8; 32]);
            }
        }
    }
    // Offset to dynamic bytes signature (relative to start of Permit2Pull).
    // 6 words PermitSingle + 1 word offset = 7 words = 224 bytes.
    out.extend_from_slice(&abi::uint_word(7 * 32));
    // Signature length + data (padded to 32-byte boundary).
    out.extend_from_slice(&abi::uint_word(signature.len() as u128));
    out.extend_from_slice(signature);
    out.extend_from_slice(&vec![0u8; (32 - signature.len() % 32) % 32]);
    out
}

/// Encode `splitSwap` calldata.
pub fn encode_split_swap(
    token_in: &str,
    token_out: &str,
    amount_in: u128,
    min_amount_out: u128,
    deadline: u64,
    sub_routes: &[BuildTxSubRoute],
    signature: &[u8],
    permit: Option<&PermitSingleFields>,
    snapshot: &MarketSnapshot,
) -> Result<String> {
    let mut routes_head = Vec::new();
    let mut routes_tail = Vec::new();
    let routes_head_len = sub_routes.len() * 32;
    for sub in sub_routes {
        let encoded = encode_sub_route(sub, snapshot)?;
        // Element offsets are relative to the start of the element-offset list;
        // each element's data follows all N offset words.
        routes_head.extend_from_slice(&abi::uint_word((routes_head_len + routes_tail.len()) as u128));
        routes_tail.extend_from_slice(&encoded);
    }
    let mut routes = Vec::new();
    routes.extend_from_slice(&abi::uint_word(sub_routes.len() as u128)); // element count
    routes.extend_from_slice(&routes_head);
    routes.extend_from_slice(&routes_tail);

    let permit_encoded = encode_permit2_pull(signature, permit);

    let mut data = Vec::new();
    data.extend_from_slice(&abi::selector(SPLIT_SWAP_SIGNATURE));
    data.extend_from_slice(&abi::address_word(token_in)?);
    data.extend_from_slice(&abi::address_word(token_out)?);
    data.extend_from_slice(&abi::uint_word(amount_in));
    data.extend_from_slice(&abi::uint_word(min_amount_out));
    data.extend_from_slice(&abi::uint_word(deadline as u128));
    // routes + permit dynamic offsets (fixed head = 7 words).
    data.extend_from_slice(&abi::uint_word(7 * 32));
    data.extend_from_slice(&abi::uint_word(7 * 32 + routes.len() as u128));
    data.extend_from_slice(&routes);
    data.extend_from_slice(&permit_encoded);

    Ok(abi::hex_with_prefix(&data))
}

/// Validate route continuity + amount sum + snapshot/factory membership.
/// Returns the resolved snapshot.
async fn validate_routes(state: &AppState, body: &BuildTxRequest) -> Result<MarketSnapshot> {
    let snapshot = load_snapshot(state).await?;
    let amount_in: u128 = body.amount_in.parse()?;
    if amount_in == 0 {
        bail!("amount_in must be positive");
    }
    let min_amount_out: u128 = body.min_amount_out.parse()?;
    if min_amount_out == 0 {
        bail!("min_amount_out must be positive");
    }
    if body.sub_routes.is_empty() {
        bail!("at least one sub-route is required");
    }

    let mut sum: u128 = 0;
    for (i, sub) in body.sub_routes.iter().enumerate() {
        let leg: u128 = sub.amount_in.parse()?;
        if leg == 0 {
            bail!("sub-route {} amount_in must be positive", i + 1);
        }
        sum = sum.checked_add(leg).ok_or_else(|| anyhow!("amount overflow"))?;
        if sub.steps.is_empty() {
            bail!("sub-route {} must have at least one step", i + 1);
        }
        let first = &sub.steps[0];
        let last = sub.steps.last().unwrap();
        if !first.token_in.eq_ignore_ascii_case(&body.token_in) {
            bail!("sub-route {} does not start with token_in", i + 1);
        }
        if !last.token_out.eq_ignore_ascii_case(&body.token_out) {
            bail!("sub-route {} does not end with token_out", i + 1);
        }
        for pair in sub.steps.windows(2) {
            if !pair[0].token_out.eq_ignore_ascii_case(&pair[1].token_in) {
                bail!("sub-route {} has a disconnected token path", i + 1);
            }
        }
        for step in &sub.steps {
            validate_hop(state, &snapshot, step).await?;
        }
    }
    if sum != amount_in {
        bail!("sub_routes amount_in sum does not match amount_in");
    }
    Ok(snapshot)
}

/// A hop's pool must exist in the snapshot with a matching venue type, and its
/// factory must be allowlisted (`chakra:factories`).
async fn validate_hop(state: &AppState, snapshot: &MarketSnapshot, step: &BuildTxStep) -> Result<()> {
    let dex_type = DexType::parse(&step.dex_type).ok_or_else(|| anyhow!("unknown dex_type: {}", step.dex_type))?;
    let pool = step.pool_address.to_ascii_lowercase();

    let known = snapshot_has_pool(snapshot, &pool, dex_type);
    if !known {
        bail!("pool {pool} not in snapshot for dex_type {}", step.dex_type);
    }
    let pool_tokens =
        snapshot_pool_tokens(snapshot, &pool, dex_type).ok_or_else(|| anyhow!("pool {pool} token topology missing"))?;
    let submitted_tokens_match = (step.token_in.eq_ignore_ascii_case(&pool_tokens.0)
        && step.token_out.eq_ignore_ascii_case(&pool_tokens.1))
        || (step.token_in.eq_ignore_ascii_case(&pool_tokens.1) && step.token_out.eq_ignore_ascii_case(&pool_tokens.0));
    if !submitted_tokens_match {
        bail!("pool {pool} tokens do not match submitted hop");
    }

    // T4.6: Validate per-hop fee when the client provides it.
    if let Some(submitted_fee) = step.fee_bps {
        if let Some(snapshot_fee) = snapshot_pool_fee(snapshot, &pool, dex_type) {
            if submitted_fee != snapshot_fee {
                bail!("pool {pool} fee mismatch: submitted {submitted_fee} bps, snapshot {snapshot_fee} bps");
            }
        }
    }

    let factories = match _pool_state_source(state) {
        Some(store) => store.fetch_factories().await.unwrap_or_default(),
        None => Vec::new(),
    };
    // The snapshot's pool record carries the factory when the worker stamped it.
    let pool_factory = snapshot_pool_factory(snapshot, &pool);
    let allowlisted = pool_factory.as_deref().map(|f| {
        factories.iter().any(|r: &FactoryRecord| {
            r.address.eq_ignore_ascii_case(f) && r.source == step_factory_source(&step.dex_type)
        })
    });
    // Legacy pools without a stamped factory: accept only when factories are
    // empty (pre-T2.5) — otherwise require membership.
    match allowlisted {
        Some(true) => Ok(()),
        Some(false) => bail!("pool {pool} factory not allowlisted in chakra:factories"),
        None if factories.is_empty() => Ok(()),
        None => bail!("pool {pool} has no factory record"),
    }
}

fn snapshot_pool_tokens(snapshot: &MarketSnapshot, pool: &str, dex_type: DexType) -> Option<(String, String)> {
    snapshot
        .sources
        .iter()
        .flat_map(|s| s.pairs.iter())
        .find(|p| p.pool_address.eq_ignore_ascii_case(pool) && p.dex_type == dex_type_name(dex_type))
        .map(|p| (p.token_a.clone(), p.token_b.clone()))
        .or_else(|| {
            snapshot
                .clmm_pool_refs
                .iter()
                .find(|p| dex_type == DexType::Clmm && p.pool_address.eq_ignore_ascii_case(pool))
                .map(|p| (p.token0.clone(), p.token1.clone()))
        })
}

fn step_factory_source(dex_type: &str) -> &'static str {
    match dex_type {
        "xyk" => "chakra-xyk",
        "stable" => "chakra-stable",
        "clmm" => "chakra-clmm",
        "xylo" => "xylo",
        _ => "",
    }
}

fn snapshot_has_pool(snapshot: &MarketSnapshot, pool: &str, dex_type: DexType) -> bool {
    snapshot
        .sources
        .iter()
        .flat_map(|s| s.pairs.iter())
        .any(|p| p.pool_address.eq_ignore_ascii_case(pool) && p.dex_type == dex_type_name(dex_type))
        || snapshot
            .clmm_pool_refs
            .iter()
            .any(|p| dex_type == DexType::Clmm && p.pool_address.eq_ignore_ascii_case(pool))
}

fn dex_type_name(dex_type: DexType) -> &'static str {
    match dex_type {
        DexType::Xyk => "xyk",
        DexType::Stable => "stable",
        DexType::Clmm => "clmm",
        DexType::Xylo => "xylo",
    }
}

fn snapshot_pool_factory(snapshot: &MarketSnapshot, pool: &str) -> Option<String> {
    snapshot
        .sources
        .iter()
        .flat_map(|s| s.pairs.iter())
        .find(|p| p.pool_address.eq_ignore_ascii_case(pool))
        .map(|p| p.factory.clone())
        .filter(|f| !f.is_empty())
        .or_else(|| {
            snapshot
                .clmm_pool_refs
                .iter()
                .find(|p| p.pool_address.eq_ignore_ascii_case(pool))
                .map(|p| p.factory.clone())
                .filter(|f| !f.is_empty())
        })
}

/// Look up the on-chain fee for a pool from the snapshot. Returns `None` when
/// the pool is not found or the snapshot doesn't carry fee data.
fn snapshot_pool_fee(snapshot: &MarketSnapshot, pool: &str, dex_type: DexType) -> Option<u32> {
    snapshot
        .sources
        .iter()
        .flat_map(|s| s.pairs.iter())
        .find(|p| p.pool_address.eq_ignore_ascii_case(pool) && p.dex_type == dex_type_name(dex_type))
        .map(|p| p.fee_bps)
        .or_else(|| {
            snapshot
                .clmm_pool_refs
                .iter()
                .find(|p| dex_type == DexType::Clmm && p.pool_address.eq_ignore_ascii_case(pool))
                .map(|p| p.fee_bps)
        })
}

async fn load_snapshot(state: &AppState) -> Result<MarketSnapshot> {
    if let Some(snapshots) = &state.memory_snapshot {
        return Ok(snapshots.load_current_snapshot().await?);
    }
    if let Some(store) = &state.snapshot_store {
        return Ok(store.load_current_snapshot().await?);
    }
    bail!("no snapshot available for build_tx validation")
}

/// Fixture-safe pool state lookups (for factory validation in cluster mode).
fn _pool_state_source(state: &AppState) -> Option<&dyn PoolStateStore> {
    state
        .pool_store
        .as_ref()
        .map(|s| s.as_ref())
        .or_else(|| state.memory_pool.as_ref().map(|s| s.as_ref() as &dyn PoolStateStore))
}

/// `paused()` on the aggregator.
async fn aggregator_paused(rpc: &EvmRpcClient, aggregator: &str) -> Result<bool> {
    let data = format!("{PAUSED_SELECTOR}");
    let word = rpc.eth_call(aggregator, &data).await?;
    let bytes = hex::decode(word.trim_start_matches("0x"))?;
    Ok(bytes.iter().any(|&b| b != 0))
}

/// ERC-20 `allowance(owner, spender)`.
async fn erc20_allowance(rpc: &EvmRpcClient, token: &str, owner: &str, spender: &str) -> Result<u128> {
    let mut data = String::from(ERC20_ALLOWANCE_SELECTOR);
    data.push_str(&hex::encode(abi::address_word(owner)?));
    data.push_str(&hex::encode(abi::address_word(spender)?));
    let word = rpc.eth_call(token, &data).await?;
    Ok(abi::word_to_u128(&hex::decode(word.trim_start_matches("0x"))?))
}

/// Permit2 AllowanceTransfer allowance: `{amount: uint160, expiration: uint48, nonce: uint48}`.
#[derive(Debug, Clone, Copy)]
pub struct Permit2Allowance {
    pub amount: u128,
    pub expiration: u64,
    pub nonce: u64,
}

/// Permit2 `allowance(user, token, spender)` (AllowanceTransfer).
/// Returns the struct as 3 ABI-encoded words: `{amount, expiration, nonce}`.
async fn permit2_allowance(rpc: &EvmRpcClient, user: &str, token: &str, spender: &str) -> Result<Permit2Allowance> {
    let mut data = String::from(PERMIT2_ALLOWANCE_SELECTOR);
    data.push_str(&hex::encode(abi::address_word(user)?));
    data.push_str(&hex::encode(abi::address_word(token)?));
    data.push_str(&hex::encode(abi::address_word(spender)?));
    let hex_resp = rpc.eth_call(PERMIT2_ADDRESS, &data).await?;
    let bytes = hex::decode(hex_resp.trim_start_matches("0x"))?;
    if bytes.len() < 96 {
        bail!("Permit2 allowance response too short");
    }
    // Allowance struct ABI-encoded as 3 separate 32-byte words:
    //   word 0 (bytes [0..32]):  amount   (uint160, right-aligned)
    //   word 1 (bytes [32..64]): expiration (uint48, right-aligned)
    //   word 2 (bytes [64..96]): nonce      (uint48, right-aligned)
    let amount = abi::word_to_u128(&bytes[0..32]);
    let expiration = abi::word_to_u128(&bytes[32..64]);
    let nonce = abi::word_to_u128(&bytes[64..96]);
    Ok(Permit2Allowance {
        amount,
        expiration: expiration as u64,
        nonce: nonce as u64,
    })
}

fn now_plus(secs: u64) -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_add(secs))
        .unwrap_or(secs)
}

/// EIP-712 Permit2 `PermitSingle` typed-data payload (AllowanceTransfer).
pub fn permit_single_typed_data(
    token: &str,
    amount: u128,
    spender: &str,
    nonce: u64,
    deadline: u64,
    chain_id: u64,
) -> Value {
    json!({
        "types": {
            "EIP712Domain": [
                {"name": "name", "type": "string"},
                {"name": "chainId", "type": "uint256"},
                {"name": "verifyingContract", "type": "address"}
            ],
            "PermitSingle": [
                {"name": "details", "type": "PermitDetails"},
                {"name": "spender", "type": "address"},
                {"name": "sigDeadline", "type": "uint256"}
            ],
            "PermitDetails": [
                {"name": "token", "type": "address"},
                {"name": "amount", "type": "uint160"},
                {"name": "expiration", "type": "uint48"},
                {"name": "nonce", "type": "uint48"}
            ]
        },
        "primaryType": "PermitSingle",
        "domain": {
            "name": "Permit2",
            "chainId": chain_id,
            "verifyingContract": PERMIT2_ADDRESS
        },
        "message": {
            "details": {
                "token": token.to_ascii_lowercase(),
                "amount": amount.to_string(),
                "expiration": deadline.to_string(),
                "nonce": nonce.to_string()
            },
            "spender": spender.to_ascii_lowercase(),
            "sigDeadline": deadline.to_string()
        }
    })
}

/// Build the full `/build_tx` response data.
pub async fn build_tx_data(
    state: &AppState,
    body: &BuildTxRequest,
) -> Result<(String, u64, Option<Value>, Vec<Value>)> {
    let aggregator = state.config.chakra_aggregator.clone();
    if aggregator.is_empty() {
        bail!(ApiErrorCode::NotReady.as_str());
    }
    let rpc = state
        .evm_rpc
        .as_ref()
        .ok_or_else(|| anyhow!(ApiErrorCode::NotReady.as_str()))?;

    validate_routes(state, body).await?;
    let snapshot = load_snapshot(state).await?;

    if aggregator_paused(rpc, &aggregator).await? {
        bail!(ApiErrorCode::Paused.as_str());
    }

    let user = body.user.trim().to_ascii_lowercase();
    let token_in = body.token_in.trim().to_ascii_lowercase();
    let amount_in: u128 = body.amount_in.parse()?;
    let min_amount_out: u128 = body.min_amount_out.parse()?;
    let deadline = now_plus(DEFAULT_DEADLINE_SECS);

    // ERC-20 allowance(user → Permit2) sufficient → no required_approvals.
    let erc20_approved = erc20_allowance(rpc, &token_in, &user, PERMIT2_ADDRESS).await? >= amount_in;
    let required_approvals: Vec<Value> = if erc20_approved {
        vec![]
    } else {
        vec![json!({
            "token": token_in,
            "spender": PERMIT2_ADDRESS,
            "amount": amount_in.to_string()
        })]
    };

    // Permit2 allowance(user, tokenIn → aggregator): check amount + expiration.
    let permit2 = permit2_allowance(rpc, &user, &token_in, &aggregator).await?;
    let permit2_sufficient = permit2.amount >= amount_in && permit2.expiration >= deadline;

    let (signature, typed_data, permit_fields) = if permit2_sufficient {
        // Allowance sufficient and unexpired — no signature needed; empty PermitSingle.
        (Vec::new(), None, None)
    } else {
        // Use the on-chain nonce and the same deadline for both typed data and
        // the encoded PermitSingle so the contract matches what the wallet signed.
        let nonce = permit2.nonce;
        let typed = permit_single_typed_data(&token_in, amount_in, &aggregator, nonce, deadline, 5042002);

        // Parse token and spender into bytes for the PermitSingle encoding.
        let token_bytes = hex::decode(token_in.trim_start_matches("0x"))?;
        let spender_bytes = hex::decode(aggregator.trim_start_matches("0x"))?;
        let permit = PermitSingleFields {
            token: token_bytes
                .try_into()
                .map_err(|_| anyhow!("invalid token address length"))?,
            amount: amount_in,
            expiration: deadline,
            nonce,
            spender: spender_bytes
                .try_into()
                .map_err(|_| anyhow!("invalid spender address length"))?,
            sig_deadline: deadline,
        };

        (Vec::new(), Some(typed), Some(permit))
    };

    let data = encode_split_swap(
        &token_in,
        &body.token_out.trim().to_ascii_lowercase(),
        amount_in,
        min_amount_out,
        deadline,
        &body.sub_routes,
        &signature,
        permit_fields.as_ref(),
        &snapshot,
    )?;

    Ok((data, deadline, typed_data, required_approvals))
}
