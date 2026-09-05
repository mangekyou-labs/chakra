//! Rebuildable Redis analytics index for confirmed Aggregator swaps.

use {
    anyhow::{bail, Context, Result},
    async_trait::async_trait,
    dex_adapters::{
        evm_logs::event_topic0_hex,
        evm_rpc::{decode_hex, EvmLog, EvmRpcClient, LogFilter},
    },
    redis::AsyncCommands,
    serde::{Deserialize, Serialize},
    std::collections::BTreeSet,
    std::sync::Arc,
    tracing::warn,
};

const SPLIT_SWAP_SELECTOR: &[u8; 4] = &[0x2e, 0x3b, 0xe0, 0xc1];
const SWAP_EVENT_SIGNATURE: &str = "Swap(address,address,address,uint256,uint256,bool)";

#[derive(Debug, Clone, Copy)]
pub struct AnalyticsConfig {
    pub enabled: bool,
    pub start_block: u64,
    pub poll_secs: u64,
    pub confirmations: u64,
    pub page_blocks: u64,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            start_block: 59_424_918,
            poll_secs: 30,
            confirmations: 12,
            page_blocks: 10_000,
        }
    }
}

impl AnalyticsConfig {
    pub fn from_env() -> Self {
        let d = Self::default();
        let num = |name: &str, fallback| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(fallback)
        };
        Self {
            enabled: std::env::var("CHAKRA_ANALYTICS_ENABLED")
                .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
                .unwrap_or(d.enabled),
            start_block: num("CHAKRA_ANALYTICS_START_BLOCK", d.start_block),
            poll_secs: num("CHAKRA_ANALYTICS_POLL_SECS", d.poll_secs).max(1),
            confirmations: num("CHAKRA_ANALYTICS_CONFIRMATIONS", d.confirmations),
            page_blocks: num("CHAKRA_ANALYTICS_PAGE_BLOCKS", d.page_blocks).max(1),
        }
    }
}

/// Confirmation-adjusted head target: the newest block whose logs are final
/// enough to index. `chain_head` is the latest block Arc reported at the
/// start of a poll; the indexer never consumes blocks newer than this.
pub fn confirmed_target(chain_head: u64, confirmations: u64) -> u64 {
    chain_head.saturating_sub(confirmations)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapSummary {
    pub tx_hash: String,
    pub trader: String,
    pub block: u64,
    pub timestamp: u64,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: String,
    pub amount_out: String,
    pub split: bool,
    pub attributed: bool,
    pub notional_micros: String,
    pub subroutes: u32,
    pub hops: u32,
    pub pools: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DecodedSplitSwap {
    pub token_in: String,
    pub token_out: String,
    pub amount_in: u128,
    pub subroutes: u32,
    pub hops: u32,
    pub pools: Vec<String>,
}

pub fn aggregator_swap_topic0() -> String {
    event_topic0_hex(SWAP_EVENT_SIGNATURE)
}

/// Decode the Aggregator's indexed Swap event. This is deliberately strict:
/// malformed logs are skipped by the indexer rather than becoming misleading
/// analytics records.
pub fn decode_swap_log(log: &EvmLog) -> Result<(String, String, String, u128, u128, bool)> {
    if log.topics.first().map(String::as_str) != Some(aggregator_swap_topic0().as_str()) {
        bail!("not an Aggregator Swap event")
    }
    if log.topics.len() < 4 {
        bail!("Swap log has fewer than three indexed topics")
    }
    let decode_address = |topic: &str| -> Result<String> {
        let bytes = decode_hex(topic)?;
        if bytes.len() != 32 {
            bail!("indexed address word must be 32 bytes")
        }
        Ok(format!("0x{}", hex::encode(&bytes[12..])))
    };
    let sender = decode_address(&log.topics[1])?;
    let token_in = decode_address(&log.topics[2])?;
    let token_out = decode_address(&log.topics[3])?;
    let data = decode_hex(&log.data)?;
    if data.len() < 96 {
        bail!("Swap data has fewer than three words")
    }
    let word_u128 = |offset: usize| -> u128 {
        let mut buf = [0u8; 16];
        buf.copy_from_slice(&data[offset + 16..offset + 32]);
        u128::from_be_bytes(buf)
    };
    Ok((sender, token_in, token_out, word_u128(0), word_u128(32), data[95] != 0))
}

/// Decode enough of splitSwap calldata to recover route topology. The nested
/// ABI uses static Hop tuples, so no external ABI runtime is required.
pub fn decode_split_swap_calldata(input: &str) -> Result<DecodedSplitSwap> {
    let bytes = decode_hex(input)?;
    if bytes.len() < 4 + 7 * 32 || &bytes[..4] != SPLIT_SWAP_SELECTOR {
        bail!("calldata is not splitSwap")
    }
    let read_word = |offset: usize| -> Result<usize> {
        if offset + 32 > bytes.len() {
            bail!("calldata word out of bounds")
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[offset + 24..offset + 32]);
        Ok(usize::try_from(u64::from_be_bytes(buf))?)
    };
    let address_word = |offset: usize| -> Result<String> {
        if offset + 32 > bytes.len() {
            bail!("calldata address out of bounds")
        }
        Ok(format!("0x{}", hex::encode(&bytes[offset + 12..offset + 32])))
    };
    let token_in = address_word(4)?;
    let token_out = address_word(36)?;
    let amount_in = read_word(68)? as u128;
    let routes = 4usize
        .checked_add(read_word(4 + 5 * 32)?)
        .context("routes offset overflow")?;
    let count = read_word(routes)?;
    if count > 64 {
        bail!("unreasonable splitSwap route count")
    }
    let mut pools = BTreeSet::new();
    let mut hops = 0u32;
    for index in 0..count {
        let offset = read_word(routes + 32 + index * 32)?;
        let subroute = routes.checked_add(32 + offset).context("subroute offset overflow")?;
        let hops_offset = read_word(subroute + 32)?;
        let hops_start = subroute.checked_add(hops_offset).context("hops offset overflow")?;
        let hop_count = read_word(hops_start)?;
        if hop_count > 64 {
            bail!("unreasonable splitSwap hop count")
        }
        hops = hops
            .checked_add(u32::try_from(hop_count)?)
            .context("hop count overflow")?;
        for hop in 0..hop_count {
            let pool = address_word(hops_start + 32 + hop * 160)?;
            pools.insert(pool);
        }
    }
    Ok(DecodedSplitSwap {
        token_in,
        token_out,
        amount_in,
        subroutes: u32::try_from(count)?,
        hops,
        pools: pools.into_iter().collect(),
    })
}

pub fn stablecoin_notional_micros(token_in: &str, amount_in: u128, token_out: &str, amount_out: u128) -> u128 {
    let stable = |token: &str| {
        token.eq_ignore_ascii_case(market_snapshot::decimals::USDC_ERC20)
            || token.eq_ignore_ascii_case(market_snapshot::decimals::EURC)
    };
    if stable(token_in) {
        amount_in
    } else if stable(token_out) {
        amount_out
    } else {
        0
    }
}

/// Redis writer. Records are keyed by transaction hash, making replay
/// idempotent; the cursor is updated only after a complete page succeeds.
pub struct AnalyticsStore {
    client: redis::Client,
    namespace: String,
}

impl AnalyticsStore {
    pub fn new(redis_url: &str, chain_id: u64, aggregator: &str) -> Result<Self> {
        Ok(Self {
            client: redis::Client::open(redis_url)?,
            namespace: format!("chakra:analytics:v1:{chain_id}:{aggregator}"),
        })
    }
    pub async fn put_swap(&self, summary: &SwapSummary) -> Result<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("{}:swap:{}", self.namespace, summary.tx_hash);
        let index = format!("{}:by_time", self.namespace);
        let payload = serde_json::to_string(summary)?;
        // SET NX and ZADD in one MULTI/EXEC so a crash cannot hide a record
        // from `by_time`. ZADD always runs so replay heals an orphaned key.
        let (set_nx, _zadd): (Option<String>, i64) = redis::pipe()
            .atomic()
            .cmd("SET")
            .arg(&key)
            .arg(&payload)
            .arg("NX")
            .cmd("ZADD")
            .arg(&index)
            .arg(summary.timestamp as f64)
            .arg(&summary.tx_hash)
            .query_async(&mut conn)
            .await?;
        Ok(set_nx.is_some())
    }
    pub async fn cursor(&self) -> Result<Option<u64>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        Ok(conn.get(format!("{}:cursor", self.namespace)).await?)
    }
    pub async fn set_cursor(&self, block: u64) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.set(format!("{}:cursor", self.namespace), block).await?;
        Ok(())
    }

    /// Record the heads for the most recent *successful* poll:
    /// `chain_head` = latest observed Arc block, `confirmed_head` =
    /// confirmation-adjusted target, `indexed_head` = last completely indexed
    /// block (the committed cursor).
    pub async fn set_heads(&self, chain_head: u64, confirmed_head: u64, indexed_head: u64) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.set(format!("{}:chain_head", self.namespace), chain_head).await?;
        let _: () = conn
            .set(format!("{}:confirmed_head", self.namespace), confirmed_head)
            .await?;
        let _: () = conn
            .set(format!("{}:indexed_head", self.namespace), indexed_head)
            .await?;
        Ok(())
    }

    /// `(chain_head, confirmed_head, indexed_head)` — only present once at
    /// least one poll has fully succeeded.
    pub async fn heads(&self) -> Result<Option<(u64, u64, u64)>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let chain: Option<u64> = conn.get(format!("{}:chain_head", self.namespace)).await?;
        let confirmed: Option<u64> = conn.get(format!("{}:confirmed_head", self.namespace)).await?;
        let indexed: Option<u64> = conn.get(format!("{}:indexed_head", self.namespace)).await?;
        Ok(chain.zip(confirmed).and_then(|(c, f)| indexed.map(|i| (c, f, i))))
    }

    /// Unix seconds of the last *fully successful* analytics poll. This is
    /// the freshness signal; it must not be confused with the age of the most
    /// recent swap (a quiet chain would otherwise look stale).
    pub async fn set_polled_at(&self, unix_secs: u64) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.set(format!("{}:polled_at", self.namespace), unix_secs).await?;
        Ok(())
    }

    pub async fn polled_at(&self) -> Result<Option<u64>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        Ok(conn.get(format!("{}:polled_at", self.namespace)).await?)
    }

    pub async fn summaries(&self, from_timestamp: u64, to_timestamp: u64) -> Result<Vec<SwapSummary>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let hashes: Vec<String> = conn
            .zrangebyscore(
                format!("{}:by_time", self.namespace),
                from_timestamp as f64,
                to_timestamp as f64,
            )
            .await?;
        let keys = hashes
            .iter()
            .map(|hash| format!("{}:swap:{}", self.namespace, hash))
            .collect::<Vec<_>>();
        let payloads: Vec<Option<String>> = if keys.is_empty() {
            vec![]
        } else {
            conn.mget(keys).await?
        };
        payloads
            .into_iter()
            .flatten()
            .map(|payload| serde_json::from_str(&payload).context("invalid analytics swap record"))
            .collect()
    }
}

/// In-memory model of Redis `SET NX` + `ZADD` for one swap. Production
/// [`AnalyticsStore::put_swap`] must run the same pair in one MULTI/EXEC.
#[cfg(test)]
pub(crate) fn index_swap(
    records: &mut std::collections::HashMap<String, String>,
    by_time: &mut std::collections::HashMap<String, u64>,
    tx_hash: &str,
    payload: String,
    timestamp: u64,
) -> bool {
    let inserted = if records.contains_key(tx_hash) {
        false
    } else {
        records.insert(tx_hash.to_string(), payload);
        true
    };
    by_time.insert(tx_hash.to_string(), timestamp);
    inserted
}

/// Backend consumed by the analytics indexer. [`AnalyticsStore`] (Redis) is
/// the production implementation; tests substitute an in-memory backend so
/// cursor, head, and freshness semantics are verified without a live Redis.
#[async_trait]
pub trait AnalyticsBackend: Send + Sync {
    async fn cursor(&self) -> Result<Option<u64>>;
    async fn set_cursor(&self, block: u64) -> Result<()>;
    async fn put_swap(&self, summary: &SwapSummary) -> Result<bool>;
    async fn set_heads(&self, chain_head: u64, confirmed_head: u64, indexed_head: u64) -> Result<()>;
    async fn set_polled_at(&self, unix_secs: u64) -> Result<()>;
    async fn heads(&self) -> Result<Option<(u64, u64, u64)>>;
    async fn polled_at(&self) -> Result<Option<u64>>;
}

#[async_trait]
impl AnalyticsBackend for AnalyticsStore {
    async fn cursor(&self) -> Result<Option<u64>> {
        AnalyticsStore::cursor(self).await
    }
    async fn set_cursor(&self, block: u64) -> Result<()> {
        AnalyticsStore::set_cursor(self, block).await
    }
    async fn put_swap(&self, summary: &SwapSummary) -> Result<bool> {
        AnalyticsStore::put_swap(self, summary).await
    }
    async fn set_heads(&self, chain_head: u64, confirmed_head: u64, indexed_head: u64) -> Result<()> {
        AnalyticsStore::set_heads(self, chain_head, confirmed_head, indexed_head).await
    }
    async fn set_polled_at(&self, unix_secs: u64) -> Result<()> {
        AnalyticsStore::set_polled_at(self, unix_secs).await
    }
    async fn heads(&self) -> Result<Option<(u64, u64, u64)>> {
        AnalyticsStore::heads(self).await
    }
    async fn polled_at(&self) -> Result<Option<u64>> {
        AnalyticsStore::polled_at(self).await
    }
}

/// Unix seconds now (for the `polled_at` freshness marker).
pub fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Confirmed-block analytics poller. A page is committed only after every log
/// has been processed; cursor advancement therefore makes interruption and
/// replay safe. Heads and the poll timestamp are written only after a *fully
/// successful* poll, which is what makes lag and freshness meaningful.
pub struct AnalyticsIndexer {
    pub config: AnalyticsConfig,
    pub rpc: Arc<EvmRpcClient>,
    pub store: Arc<dyn AnalyticsBackend>,
    pub aggregator: String,
}

impl AnalyticsIndexer {
    pub fn new(
        config: AnalyticsConfig,
        rpc: Arc<EvmRpcClient>,
        store: Arc<dyn AnalyticsBackend>,
        aggregator: String,
    ) -> Self {
        Self {
            config,
            rpc,
            store,
            aggregator: aggregator.to_ascii_lowercase(),
        }
    }

    /// Returns `(chain_head, confirmed_head, indexed_head, new_records)`.
    pub async fn poll_once(&self) -> Result<(u64, u64, u64, usize)> {
        let chain_head = self.rpc.eth_block_number().await?;
        let confirmed_head = confirmed_target(chain_head, self.config.confirmations);
        let mut cursor = self
            .store
            .cursor()
            .await?
            .unwrap_or(self.config.start_block.saturating_sub(1));
        if cursor >= confirmed_head {
            // Nothing new to index; the poll still succeeded, so refresh the
            // heads (the chain may have advanced) and the freshness marker.
            self.store.set_heads(chain_head, confirmed_head, cursor).await?;
            self.store.set_polled_at(unix_now_secs()).await?;
            return Ok((chain_head, confirmed_head, cursor, 0));
        }
        let mut indexed = 0usize;
        while cursor < confirmed_head {
            let mut page = self.config.page_blocks.min(confirmed_head - cursor);
            let (logs, end) = loop {
                let filter = LogFilter {
                    from_block: Some(cursor + 1),
                    to_block: Some(cursor + page),
                    addresses: vec![self.aggregator.clone()],
                    topics: vec![Some(vec![aggregator_swap_topic0()])],
                };
                match self.rpc.eth_get_logs(&filter).await {
                    Ok(logs) => break (logs, cursor + page),
                    Err(error) if page > 1 => {
                        page = (page / 2).max(1);
                        warn!(from = cursor + 1, to = cursor + page, %error, "reducing analytics RPC page");
                    }
                    Err(error) => return Err(error),
                }
            };
            for log in logs {
                let Some(block) = log.block_number else {
                    continue;
                };
                let Ok((sender, token_in, token_out, amount_in, amount_out, split)) = decode_swap_log(&log) else {
                    continue;
                };
                let timestamp = self.rpc.eth_get_block_timestamp(block).await.unwrap_or(0);
                let mut summary = SwapSummary {
                    tx_hash: log.tx_hash.clone().unwrap_or_default(),
                    trader: sender,
                    block,
                    timestamp,
                    token_in: token_in.clone(),
                    token_out: token_out.clone(),
                    amount_in: amount_in.to_string(),
                    amount_out: amount_out.to_string(),
                    split,
                    attributed: false,
                    notional_micros: stablecoin_notional_micros(&token_in, amount_in, &token_out, amount_out)
                        .to_string(),
                    subroutes: 0,
                    hops: 0,
                    pools: vec![],
                };
                if let Some(tx_hash) = log.tx_hash.as_deref() {
                    if let Ok(Some(input)) = self.rpc.eth_get_transaction_input(tx_hash).await {
                        if let Ok(decoded) = decode_split_swap_calldata(&input) {
                            summary.attributed = true;
                            summary.split = decoded.subroutes > 1 || summary.split;
                            summary.subroutes = decoded.subroutes;
                            summary.hops = decoded.hops;
                            summary.pools = decoded.pools;
                        }
                    }
                }
                if !summary.tx_hash.is_empty() && self.store.put_swap(&summary).await? {
                    indexed += 1;
                }
            }
            // A page is committed only after every log in it was processed;
            // interruption before this leaves the cursor behind so a replay
            // re-fetches the exact same window (records are idempotent).
            self.store.set_cursor(end).await?;
            cursor = end;
        }
        self.store.set_heads(chain_head, confirmed_head, cursor).await?;
        self.store.set_polled_at(unix_now_secs()).await?;
        Ok((chain_head, confirmed_head, cursor, indexed))
    }

    pub async fn run(self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(self.config.poll_secs));
        loop {
            interval.tick().await;
            if let Err(error) = self.poll_once().await {
                warn!(%error, "analytics poll failed; cursor retained");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(value: usize) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[24..].copy_from_slice(&(value as u64).to_be_bytes());
        out
    }
    #[test]
    fn notional_uses_stable_input_or_output() {
        assert_eq!(
            stablecoin_notional_micros(
                market_snapshot::decimals::USDC_ERC20,
                7,
                market_snapshot::decimals::CIRBTC,
                9
            ),
            7
        );
        assert_eq!(
            stablecoin_notional_micros(market_snapshot::decimals::CIRBTC, 9, market_snapshot::decimals::EURC, 8),
            8
        );
        assert_eq!(
            stablecoin_notional_micros(
                market_snapshot::decimals::CIRBTC,
                9,
                market_snapshot::decimals::CIRBTC,
                8
            ),
            0
        );
    }
    #[test]
    fn config_defaults_match_release_contract() {
        let c = AnalyticsConfig::default();
        assert_eq!(
            (c.start_block, c.poll_secs, c.confirmations, c.page_blocks),
            (59_424_918, 30, 12, 10_000)
        );
    }

    #[test]
    fn decodes_swap_event_and_nested_route_topology() {
        let topic = aggregator_swap_topic0();
        let indexed = |address: &str| format!("0x{}", "0".repeat(24) + address.trim_start_matches("0x"));
        let mut data = vec![0u8; 96];
        data[31] = 7;
        data[63] = 9;
        data[95] = 1;
        let log = EvmLog {
            address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            topics: vec![
                topic,
                indexed("0x1111111111111111111111111111111111111111"),
                indexed("0x2222222222222222222222222222222222222222"),
                indexed("0x3333333333333333333333333333333333333333"),
            ],
            data: format!("0x{}", hex::encode(data)),
            block_number: Some(1),
            tx_hash: Some("0xtx".into()),
            log_index: Some(0),
        };
        let (_, token_in, token_out, amount_in, amount_out, split) = decode_swap_log(&log).unwrap();
        assert_eq!(token_in, "0x2222222222222222222222222222222222222222");
        assert_eq!(token_out, "0x3333333333333333333333333333333333333333");
        assert_eq!((amount_in, amount_out, split), (7, 9, true));

        let mut calldata = vec![0u8; 548];
        calldata[..4].copy_from_slice(SPLIT_SWAP_SELECTOR);
        calldata[4 + 12..4 + 32].copy_from_slice(&[0x22; 20]);
        calldata[36 + 12..36 + 32].copy_from_slice(&[0x33; 20]);
        calldata[68 + 31] = 7;
        calldata[4 + 5 * 32..4 + 6 * 32].copy_from_slice(&word(224));
        let routes = 4 + 224;
        calldata[routes..routes + 32].copy_from_slice(&word(1));
        calldata[routes + 32..routes + 64].copy_from_slice(&word(32));
        let subroute = routes + 64;
        calldata[subroute + 32..subroute + 64].copy_from_slice(&word(64));
        let hops = subroute + 64;
        calldata[hops..hops + 32].copy_from_slice(&word(1));
        calldata[hops + 32 + 12..hops + 32 + 32].copy_from_slice(&[0x44; 20]);
        let decoded = decode_split_swap_calldata(&format!("0x{}", hex::encode(calldata))).unwrap();
        assert_eq!((decoded.subroutes, decoded.hops), (1, 1));
        assert_eq!(decoded.pools, vec!["0x4444444444444444444444444444444444444444"]);
    }
    // ─── Poll semantics (in-memory backend; no Redis required) ───────

    struct MemoryBackend {
        cursor: std::sync::Mutex<Option<u64>>,
        heads: std::sync::Mutex<Option<(u64, u64, u64)>>,
        polled_at: std::sync::Mutex<Option<u64>>,
        swaps: std::sync::Mutex<std::collections::HashSet<String>>,
    }

    impl MemoryBackend {
        fn new() -> Self {
            Self {
                cursor: std::sync::Mutex::new(None),
                heads: std::sync::Mutex::new(None),
                polled_at: std::sync::Mutex::new(None),
                swaps: std::sync::Mutex::new(std::collections::HashSet::new()),
            }
        }
    }

    #[async_trait]
    impl AnalyticsBackend for MemoryBackend {
        async fn cursor(&self) -> Result<Option<u64>> {
            Ok(*self.cursor.lock().unwrap())
        }
        async fn set_cursor(&self, block: u64) -> Result<()> {
            *self.cursor.lock().unwrap() = Some(block);
            Ok(())
        }
        async fn put_swap(&self, summary: &SwapSummary) -> Result<bool> {
            Ok(self.swaps.lock().unwrap().insert(summary.tx_hash.clone()))
        }
        async fn set_heads(&self, chain_head: u64, confirmed_head: u64, indexed_head: u64) -> Result<()> {
            *self.heads.lock().unwrap() = Some((chain_head, confirmed_head, indexed_head));
            Ok(())
        }
        async fn set_polled_at(&self, unix_secs: u64) -> Result<()> {
            *self.polled_at.lock().unwrap() = Some(unix_secs);
            Ok(())
        }
        async fn heads(&self) -> Result<Option<(u64, u64, u64)>> {
            Ok(*self.heads.lock().unwrap())
        }
        async fn polled_at(&self) -> Result<Option<u64>> {
            Ok(*self.polled_at.lock().unwrap())
        }
    }

    #[test]
    fn confirmed_target_is_chain_head_minus_confirmations() {
        assert_eq!(confirmed_target(200, 5), 195);
        assert_eq!(confirmed_target(4, 12), 0, "small chains saturate at zero");
        assert_eq!(confirmed_target(0, 12), 0);
    }

    #[test]
    fn index_swap_inserts_the_record_and_the_time_index() {
        let mut records = std::collections::HashMap::new();
        let mut by_time = std::collections::HashMap::new();
        assert!(index_swap(
            &mut records,
            &mut by_time,
            "0xtx",
            "{\"tx\":\"0xtx\"}".into(),
            1_700_000_000,
        ));
        assert_eq!(records.get("0xtx").map(String::as_str), Some("{\"tx\":\"0xtx\"}"));
        assert_eq!(by_time.get("0xtx").copied(), Some(1_700_000_000));
    }

    #[test]
    fn index_swap_replay_is_idempotent_and_does_not_overwrite_payload() {
        let mut records = std::collections::HashMap::from([("0xtx".into(), "first".into())]);
        let mut by_time = std::collections::HashMap::from([("0xtx".into(), 1)]);
        assert!(!index_swap(&mut records, &mut by_time, "0xtx", "second".into(), 2));
        assert_eq!(records.get("0xtx").map(String::as_str), Some("first"));
        assert_eq!(by_time.get("0xtx").copied(), Some(2), "replay still refreshes by_time");
    }

    #[test]
    fn index_swap_heals_a_missing_by_time_entry_when_the_record_already_exists() {
        let mut records = std::collections::HashMap::from([("0xtx".into(), "payload".into())]);
        let mut by_time = std::collections::HashMap::new();
        assert!(!index_swap(&mut records, &mut by_time, "0xtx", "ignored".into(), 42,));
        assert_eq!(records.get("0xtx").map(String::as_str), Some("payload"));
        assert_eq!(
            by_time.get("0xtx").copied(),
            Some(42),
            "crash between SET NX and ZADD must not hide the swap from by_time after replay"
        );
    }

    #[tokio::test]
    async fn poll_records_heads_cursor_and_freshness_after_catch_up() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
        let calls_inside = calls.clone();
        let (url, _server) = crate::evm_watcher::tests::spawn_fixture_rpc(move |method, params| {
            match method {
                "eth_blockNumber" => Ok(serde_json::json!("0xc8")), // block 200
                "eth_getLogs" => {
                    let from = params[0]["fromBlock"].as_str().unwrap().to_string();
                    let to = params[0]["toBlock"].as_str().unwrap().to_string();
                    calls_inside.lock().unwrap().push((from, to));
                    Ok(serde_json::json!([]))
                }
                other => Err(serde_json::json!(format!("unexpected method {other}"))),
            }
        });
        let backend = Arc::new(MemoryBackend::new()) as Arc<dyn AnalyticsBackend>;
        let indexer = AnalyticsIndexer::new(
            AnalyticsConfig {
                enabled: false,
                start_block: 100,
                poll_secs: 30,
                confirmations: 5,
                page_blocks: 50,
            },
            Arc::new(EvmRpcClient::single(&url).unwrap()),
            backend.clone(),
            "0xaggregator".to_string(),
        );
        let (chain, confirmed, indexed, new_records) = indexer.poll_once().await.unwrap();
        assert_eq!((chain, confirmed, indexed, new_records), (200, 195, 195, 0));
        {
            let guard = calls.lock().unwrap();
            assert_eq!(
                guard.as_slice(),
                &[
                    ("0x64".to_string(), "0x95".to_string()), // 100..=149 (50 blocks)
                    ("0x96".to_string(), "0xc3".to_string()), // 150..=195 (46 blocks)
                ],
                "paging must walk the confirmed window in bounded pages"
            );
        }
        // Semantics contract: indexed_head == cursor == confirmed target,
        // chain_head == latest observed block, polled_at == last success.
        assert_eq!(backend.cursor().await.unwrap(), Some(195));
        assert_eq!(backend.heads().await.unwrap(), Some((200, 195, 195)));
        let polled = backend.polled_at().await.unwrap().expect("polled_at written");
        assert!(polled <= unix_now_secs() && polled + 5 >= unix_now_secs());
    }

    #[tokio::test]
    async fn failed_page_keeps_cursor_heads_and_freshness_untouched() {
        let fail_pages = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let flag = fail_pages.clone();
        let (url, _server) = crate::evm_watcher::tests::spawn_fixture_rpc(move |method, params| {
            match method {
                "eth_blockNumber" => Ok(serde_json::json!("0xc8")),
                "eth_getLogs" => {
                    let from = params[0]["fromBlock"].as_str().unwrap().to_string();
                    // 151 (0x97) is the start of the second page.
                    if flag.load(std::sync::atomic::Ordering::Relaxed) && from.as_str() >= "0x96" {
                        Err(serde_json::json!("simulated RPC outage"))
                    } else {
                        Ok(serde_json::json!([]))
                    }
                }
                other => Err(serde_json::json!(format!("unexpected method {other}"))),
            }
        });
        let backend: Arc<dyn AnalyticsBackend> = Arc::new(MemoryBackend::new());
        let indexer = AnalyticsIndexer::new(
            AnalyticsConfig {
                enabled: false,
                start_block: 100,
                poll_secs: 30,
                confirmations: 5,
                page_blocks: 50,
            },
            Arc::new(EvmRpcClient::single(&url).unwrap()),
            backend.clone(),
            "0xaggregator".to_string(),
        );
        assert!(indexer.poll_once().await.is_err(), "page failure must fail the poll");
        // The committed first page advanced the cursor to 149, but heads
        // and the freshness marker must not move until the whole poll succeeds.
        assert_eq!(backend.cursor().await.unwrap(), Some(149));
        assert_eq!(backend.heads().await.unwrap(), None);
        assert!(backend.polled_at().await.unwrap().is_none());
        // Replay is safe: the next poll resumes at the committed cursor.
        fail_pages.store(false, std::sync::atomic::Ordering::Relaxed);
        let (chain, confirmed, indexed, _) = indexer.poll_once().await.unwrap();
        assert_eq!((chain, confirmed, indexed), (200, 195, 195));
        assert_eq!(backend.cursor().await.unwrap(), Some(195));
        assert_eq!(backend.heads().await.unwrap(), Some((200, 195, 195)));
        assert!(backend.polled_at().await.unwrap().is_some());
    }
}
