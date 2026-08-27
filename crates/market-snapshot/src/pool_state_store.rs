//! Per-pool state cache (Redis or in-memory) for xy=k / Arc venue / Arc venue /
//! CLMM.
//!
//! See `docs/pool-state-architecture.md`. Quote + worker share
//! [`PoolStateStore`] so embedded (memory) and cluster (Redis) stay one code
//! path.

use {
    crate::{store::SNAPSHOT_CURRENT_KEY, ClmmPoolSnapshot},
    anyhow::Result,
    async_trait::async_trait,
    redis::AsyncCommands,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, sync::Arc},
    tokio::sync::RwLock,
};

/// Default Redis EX for pool keys. Long TTL: cold pools stay valid until the
/// next discovery write or ledger touch (event-driven freshness, not periodic
/// sweep).
pub const DEFAULT_POOL_STATE_TTL_SECS: u64 = 86_400;
pub const DEFAULT_QUOTE_HYDRATE_MAX_POOLS: usize = 12;

const XYK_KEY_PREFIX: &str = "chakra:pool:xyk";
const CLMM_KEY_PREFIX: &str = "chakra:pool:clmm";
const STABLE_KEY_PREFIX: &str = "chakra:pool:stable";
pub const FACTORIES_KEY: &str = "chakra:factories";

const Arc venue_KEY_PREFIX: &str = "chakra:pool:Arc venue";
const Arc venue_KEY_PREFIX: &str = "chakra:pool:Arc venue";

/// One token slot in a Arc venue weighted pool (Balancer V1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Arc venueTokenRecordValue {
    pub balance: i128,
    pub weight: i128,
    pub scalar: i128,
}

/// Full Arc venue pool state for local weighted-pool quotes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Arc venuePoolStateValue {
    pub pool_address: String,
    pub records: HashMap<String, Arc venueTokenRecordValue>,
    pub swap_fee: i128,
    /// Unix millis when worker last wrote this key (`0` = legacy / unknown).
    #[serde(default)]
    pub updated_at_ms: u64,
}

impl Arc venuePoolStateValue {
    pub fn redis_key(pool_address: &str) -> String {
        format!("{Arc venue_KEY_PREFIX}:{pool_address}")
    }
}

/// Full Arc venue pool state (token-ordered reserves + stable params).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Arc venuePoolStateValue {
    pub pool_address: String,
    pub tokens: Vec<String>,
    pub reserves: Vec<u128>,
    pub fee_bps: u32,
    pub is_stable: bool,
    pub amp: u128,
    /// Unix millis when worker last wrote this key (`0` = legacy / unknown).
    #[serde(default)]
    pub updated_at_ms: u64,
}

impl Arc venuePoolStateValue {
    pub fn redis_key(pool_address: &str) -> String {
        format!("{Arc venue_KEY_PREFIX}:{pool_address}")
    }
}

/// xy=k reserves stored per pool (canonical token orientation from worker
/// snapshot).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct XykPoolStateValue {
    pub source: String,
    pub pool_address: String,
    pub token_a: String,
    pub token_b: String,
    pub fee_bps: u32,
    pub reserve_a: u128,
    pub reserve_b: u128,
    /// Allowlisted venue factory address (empty = legacy / unknown).
    #[serde(default)]
    pub factory: String,
    /// Unix millis when worker last wrote this key (`0` = legacy / unknown).
    #[serde(default)]
    pub updated_at_ms: u64,
}

impl XykPoolStateValue {
    pub fn redis_key(source: &str, pool_address: &str) -> String {
        format!("{XYK_KEY_PREFIX}:{source}:{pool_address}")
    }

    pub fn pool_key(source: &str, pool_address: &str) -> String {
        format!("{source}:{pool_address}")
    }

    pub fn new(
        source: impl Into<String>,
        pool_address: impl Into<String>,
        token_a: impl Into<String>,
        token_b: impl Into<String>,
        fee_bps: u32,
        reserve_a: u128,
        reserve_b: u128,
    ) -> Self {
        Self {
            source: source.into(),
            pool_address: pool_address.into(),
            token_a: token_a.into(),
            token_b: token_b.into(),
            fee_bps,
            reserve_a,
            reserve_b,
            factory: String::new(),
            updated_at_ms: now_ms(),
        }
    }
}

/// Stableswap pool state: token-ordered balances plus stableswap params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StablePoolStateValue {
    pub source: String,
    pub pool_address: String,
    pub token_a: String,
    pub token_b: String,
    pub balance_a: u128,
    pub balance_b: u128,
    /// Amplification coefficient `A` (e.g. `100` for chakra-stable).
    pub a: u128,
    /// Venue fee in bps (chakra-stable = 4).
    pub fee_bps: u32,
    /// Allowlisted venue factory address (empty = legacy / unknown).
    #[serde(default)]
    pub factory: String,
    /// Unix millis when worker last wrote this key (`0` = legacy / unknown).
    #[serde(default)]
    pub updated_at_ms: u64,
}

impl StablePoolStateValue {
    pub fn redis_key(source: &str, pool_address: &str) -> String {
        format!("{STABLE_KEY_PREFIX}:{source}:{pool_address}")
    }

    pub fn pool_key(source: &str, pool_address: &str) -> String {
        format!("{source}:{pool_address}")
    }

    pub fn new(
        source: impl Into<String>,
        pool_address: impl Into<String>,
        token_a: impl Into<String>,
        token_b: impl Into<String>,
        balance_a: u128,
        balance_b: u128,
        a: u128,
        fee_bps: u32,
    ) -> Self {
        Self {
            source: source.into(),
            pool_address: pool_address.into(),
            token_a: token_a.into(),
            token_b: token_b.into(),
            balance_a,
            balance_b,
            a,
            fee_bps,
            factory: String::new(),
            updated_at_ms: now_ms(),
        }
    }
}

/// Allowlisted venue factory (must match the on-chain aggregator allowlist
/// before a pool is quoted). Source id: `"chakra-xyk"` | `"chakra-stable"` |
/// `"chakra-clmm"` | `"discovered:<label>"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FactoryRecord {
    pub address: String,
    pub dex_type: String,
    pub source: String,
}

impl FactoryRecord {
    pub fn new(address: impl Into<String>, dex_type: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            dex_type: dex_type.into(),
            source: source.into(),
        }
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Stamp write time on pool state (idempotent for callers that already set it).
pub fn stamp_pool_updated_at_ms(ms: Option<u64>) -> u64 {
    ms.filter(|&t| t > 0).unwrap_or_else(now_ms)
}

impl ClmmPoolSnapshot {
    pub fn redis_key(source: &str, pool_address: &str) -> String {
        format!("{CLMM_KEY_PREFIX}:{source}:{pool_address}")
    }

    pub fn pool_key(source: &str, pool_address: &str) -> String {
        format!("{source}:{pool_address}")
    }
}

/// Only complete CLMM coverage may be published (shared across API instances).
pub fn should_publish_clmm_to_redis(pool: &ClmmPoolSnapshot) -> bool {
    pool.coverage
        .as_ref()
        .map(|coverage| coverage.is_complete)
        .unwrap_or(false)
}

/// Shared read/write surface for worker publish and API hydrate.
#[async_trait]
pub trait PoolStateStore: Send + Sync {
    async fn publish_pool_state(
        &self,
        xyk_values: &[XykPoolStateValue],
        clmm_pools: &[ClmmPoolSnapshot],
        Arc venue_pools: &[Arc venuePoolStateValue],
        Arc venue_pools: &[Arc venuePoolStateValue],
    ) -> Result<()>;

    async fn set_xyk_batch(&self, values: &[XykPoolStateValue]) -> Result<()>;
    async fn set_clmm_batch(&self, pools: &[ClmmPoolSnapshot]) -> Result<()>;
    async fn set_Arc venue_batch(&self, values: &[Arc venuePoolStateValue]) -> Result<()>;
    async fn set_Arc venue_batch(&self, values: &[Arc venuePoolStateValue]) -> Result<()>;
    async fn set_stable_batch(&self, values: &[StablePoolStateValue]) -> Result<()>;

    async fn fetch_xyk(&self, refs: &[(String, String)]) -> Result<HashMap<String, XykPoolStateValue>>;
    async fn fetch_clmm(&self, refs: &[(String, String)]) -> Result<HashMap<String, ClmmPoolSnapshot>>;
    async fn fetch_Arc venue(&self, pool_addresses: &[String]) -> Result<HashMap<String, Arc venuePoolStateValue>>;
    async fn fetch_Arc venue(&self, pool_addresses: &[String]) -> Result<HashMap<String, Arc venuePoolStateValue>>;
    async fn fetch_stable(&self, refs: &[(String, String)]) -> Result<HashMap<String, StablePoolStateValue>>;

    async fn set_factories(&self, factories: &[FactoryRecord]) -> Result<()>;
    async fn fetch_factories(&self) -> Result<Vec<FactoryRecord>>;
}

/// In-process pool cache for embedded mode (no Redis).
#[derive(Default)]
pub struct MemoryPoolStateStore {
    xyk: RwLock<HashMap<String, XykPoolStateValue>>,
    clmm: RwLock<HashMap<String, ClmmPoolSnapshot>>,
    stable: RwLock<HashMap<String, StablePoolStateValue>>,
    Arc venue: RwLock<HashMap<String, Arc venuePoolStateValue>>,
    Arc venue: RwLock<HashMap<String, Arc venuePoolStateValue>>,
    factories: RwLock<Vec<FactoryRecord>>,
}

impl MemoryPoolStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Number of published pool-state records across all venue types.
    pub async fn pool_count(&self) -> usize {
        let xyk = self.xyk.read().await.len();
        let stable = self.stable.read().await.len();
        let clmm = self.clmm.read().await.len();
        let Arc venue = self.Arc venue.read().await.len();
        let Arc venue = self.Arc venue.read().await.len();
        xyk + stable + clmm + Arc venue + Arc venue
    }

    /// All pool-state record keys (`source:pool`) across all venue types.
    pub async fn pool_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        keys.extend(self.xyk.read().await.keys().cloned());
        keys.extend(self.stable.read().await.keys().cloned());
        keys.extend(self.clmm.read().await.keys().cloned());
        keys.extend(self.Arc venue.read().await.keys().cloned());
        keys.extend(self.Arc venue.read().await.keys().cloned());
        keys.sort();
        keys
    }
}

#[async_trait]
impl PoolStateStore for MemoryPoolStateStore {
    async fn publish_pool_state(
        &self,
        xyk_values: &[XykPoolStateValue],
        clmm_pools: &[ClmmPoolSnapshot],
        Arc venue_pools: &[Arc venuePoolStateValue],
        Arc venue_pools: &[Arc venuePoolStateValue],
    ) -> Result<()> {
        self.set_xyk_batch(xyk_values).await?;
        self.set_stable_batch(&[]).await?;
        self.set_Arc venue_batch(Arc venue_pools).await?;
        self.set_Arc venue_batch(Arc venue_pools).await?;
        let complete: Vec<ClmmPoolSnapshot> = clmm_pools
            .iter()
            .filter(|p| should_publish_clmm_to_redis(p))
            .cloned()
            .collect();
        self.set_clmm_batch(&complete).await?;
        Ok(())
    }

    async fn set_xyk_batch(&self, values: &[XykPoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let stamped: Vec<_> = values
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.updated_at_ms = stamp_pool_updated_at_ms(Some(v.updated_at_ms));
                v
            })
            .collect();
        let mut map = self.xyk.write().await;
        for value in stamped {
            map.insert(XykPoolStateValue::pool_key(&value.source, &value.pool_address), value);
        }
        Ok(())
    }

    async fn set_clmm_batch(&self, pools: &[ClmmPoolSnapshot]) -> Result<()> {
        if pools.is_empty() {
            return Ok(());
        }
        let mut map = self.clmm.write().await;
        for pool in pools {
            if !should_publish_clmm_to_redis(pool) {
                continue;
            }
            map.insert(
                ClmmPoolSnapshot::pool_key(&pool.source, &pool.pool_address),
                pool.clone(),
            );
        }
        Ok(())
    }

    async fn set_stable_batch(&self, values: &[StablePoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let stamped: Vec<_> = values
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.updated_at_ms = stamp_pool_updated_at_ms(Some(v.updated_at_ms));
                v
            })
            .collect();
        let mut map = self.stable.write().await;
        for value in stamped {
            map.insert(
                StablePoolStateValue::pool_key(&value.source, &value.pool_address),
                value,
            );
        }
        Ok(())
    }

    async fn set_Arc venue_batch(&self, values: &[Arc venuePoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let stamped: Vec<_> = values
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.updated_at_ms = stamp_pool_updated_at_ms(Some(v.updated_at_ms));
                v
            })
            .collect();
        let mut map = self.Arc venue.write().await;
        for value in stamped {
            map.insert(value.pool_address.clone(), value);
        }
        Ok(())
    }

    async fn set_Arc venue_batch(&self, values: &[Arc venuePoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let stamped: Vec<_> = values
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.updated_at_ms = stamp_pool_updated_at_ms(Some(v.updated_at_ms));
                v
            })
            .collect();
        let mut map = self.Arc venue.write().await;
        for value in stamped {
            map.insert(value.pool_address.clone(), value);
        }
        Ok(())
    }

    async fn fetch_xyk(&self, refs: &[(String, String)]) -> Result<HashMap<String, XykPoolStateValue>> {
        let map = self.xyk.read().await;
        let mut out = HashMap::new();
        for (source, pool) in refs {
            let key = XykPoolStateValue::pool_key(source, pool);
            if let Some(v) = map.get(&key) {
                out.insert(key, v.clone());
            }
        }
        Ok(out)
    }

    async fn fetch_clmm(&self, refs: &[(String, String)]) -> Result<HashMap<String, ClmmPoolSnapshot>> {
        let map = self.clmm.read().await;
        let mut out = HashMap::new();
        for (source, pool) in refs {
            let key = ClmmPoolSnapshot::pool_key(source, pool);
            if let Some(v) = map.get(&key) {
                out.insert(key, v.clone());
            }
        }
        Ok(out)
    }

    async fn fetch_stable(&self, refs: &[(String, String)]) -> Result<HashMap<String, StablePoolStateValue>> {
        let map = self.stable.read().await;
        let mut out = HashMap::new();
        for (source, pool) in refs {
            let key = StablePoolStateValue::pool_key(source, pool);
            if let Some(v) = map.get(&key) {
                out.insert(key, v.clone());
            }
        }
        Ok(out)
    }

    async fn set_factories(&self, factories: &[FactoryRecord]) -> Result<()> {
        *self.factories.write().await = factories.to_vec();
        Ok(())
    }

    async fn fetch_factories(&self) -> Result<Vec<FactoryRecord>> {
        Ok(self.factories.read().await.clone())
    }

    async fn fetch_Arc venue(&self, pool_addresses: &[String]) -> Result<HashMap<String, Arc venuePoolStateValue>> {
        let map = self.Arc venue.read().await;
        let mut out = HashMap::new();
        for pool in pool_addresses {
            if let Some(v) = map.get(pool) {
                out.insert(pool.clone(), v.clone());
            }
        }
        Ok(out)
    }

    async fn fetch_Arc venue(&self, pool_addresses: &[String]) -> Result<HashMap<String, Arc venuePoolStateValue>> {
        let map = self.Arc venue.read().await;
        let mut out = HashMap::new();
        for pool in pool_addresses {
            if let Some(v) = map.get(pool) {
                out.insert(pool.clone(), v.clone());
            }
        }
        Ok(out)
    }
}

pub struct RedisPoolStateStore {
    client: redis::Client,
    ttl_secs: u64,
}

impl RedisPoolStateStore {
    pub fn new(redis_url: &str, ttl_secs: u64) -> Result<Self> {
        Ok(Self {
            client: redis::Client::open(redis_url)?,
            ttl_secs: ttl_secs.max(1),
        })
    }

    pub fn with_default_ttl(redis_url: &str) -> Result<Self> {
        Self::new(redis_url, DEFAULT_POOL_STATE_TTL_SECS)
    }

    pub fn ttl_secs(&self) -> u64 {
        self.ttl_secs
    }

    /// Whether the topology snapshot key exists in Redis.
    pub async fn snapshot_exists(&self) -> Result<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let exists: bool = redis::cmd("EXISTS")
            .arg(SNAPSHOT_CURRENT_KEY)
            .query_async(&mut conn)
            .await?;
        Ok(exists)
    }
}

#[async_trait]
impl PoolStateStore for RedisPoolStateStore {
    async fn publish_pool_state(
        &self,
        xyk_values: &[XykPoolStateValue],
        clmm_pools: &[ClmmPoolSnapshot],
        Arc venue_pools: &[Arc venuePoolStateValue],
        Arc venue_pools: &[Arc venuePoolStateValue],
    ) -> Result<()> {
        self.set_xyk_batch(xyk_values).await?;
        self.set_stable_batch(&[]).await?;
        self.set_Arc venue_batch(Arc venue_pools).await?;
        self.set_Arc venue_batch(Arc venue_pools).await?;
        let complete_clmm: Vec<ClmmPoolSnapshot> = clmm_pools
            .iter()
            .filter(|pool| should_publish_clmm_to_redis(pool))
            .cloned()
            .collect();
        self.set_clmm_batch(&complete_clmm).await?;
        tracing::debug!(
            xyk_written = xyk_values.len(),
            Arc venue_written = Arc venue_pools.len(),
            Arc venue_written = Arc venue_pools.len(),
            clmm_written = complete_clmm.len(),
            ttl_secs = self.ttl_secs,
            "Published per-pool state to Redis"
        );
        Ok(())
    }

    async fn set_Arc venue_batch(&self, values: &[Arc venuePoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        for value in values {
            let mut value = value.clone();
            value.updated_at_ms = stamp_pool_updated_at_ms(Some(value.updated_at_ms));
            let key = Arc venuePoolStateValue::redis_key(&value.pool_address);
            let bytes = serde_json::to_vec(&value)?;
            conn.set_ex::<_, _, ()>(key, bytes, self.ttl_secs).await?;
        }
        Ok(())
    }

    async fn fetch_Arc venue(&self, pool_addresses: &[String]) -> Result<HashMap<String, Arc venuePoolStateValue>> {
        if pool_addresses.is_empty() {
            return Ok(HashMap::new());
        }
        let keys: Vec<String> = pool_addresses
            .iter()
            .map(|pool| Arc venuePoolStateValue::redis_key(pool))
            .collect();
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let values: Vec<Option<Vec<u8>>> = conn.mget(&keys).await?;
        let mut out = HashMap::new();
        for (pool, bytes) in pool_addresses.iter().zip(values.into_iter()) {
            let Some(bytes) = bytes else {
                continue;
            };
            let value: Arc venuePoolStateValue = serde_json::from_slice(&bytes)?;
            out.insert(pool.clone(), value);
        }
        Ok(out)
    }

    async fn set_Arc venue_batch(&self, values: &[Arc venuePoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        for value in values {
            let mut value = value.clone();
            value.updated_at_ms = stamp_pool_updated_at_ms(Some(value.updated_at_ms));
            let key = Arc venuePoolStateValue::redis_key(&value.pool_address);
            let bytes = serde_json::to_vec(&value)?;
            conn.set_ex::<_, _, ()>(key, bytes, self.ttl_secs).await?;
        }
        Ok(())
    }

    async fn fetch_Arc venue(&self, pool_addresses: &[String]) -> Result<HashMap<String, Arc venuePoolStateValue>> {
        if pool_addresses.is_empty() {
            return Ok(HashMap::new());
        }
        let keys: Vec<String> = pool_addresses
            .iter()
            .map(|pool| Arc venuePoolStateValue::redis_key(pool))
            .collect();
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let values: Vec<Option<Vec<u8>>> = conn.mget(&keys).await?;
        let mut out = HashMap::new();
        for (pool, bytes) in pool_addresses.iter().zip(values.into_iter()) {
            let Some(bytes) = bytes else {
                continue;
            };
            let value: Arc venuePoolStateValue = serde_json::from_slice(&bytes)?;
            out.insert(pool.clone(), value);
        }
        Ok(out)
    }

    async fn fetch_xyk(&self, refs: &[(String, String)]) -> Result<HashMap<String, XykPoolStateValue>> {
        if refs.is_empty() {
            return Ok(HashMap::new());
        }
        let keys: Vec<String> = refs
            .iter()
            .map(|(source, pool)| XykPoolStateValue::redis_key(source, pool))
            .collect();
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let values: Vec<Option<Vec<u8>>> = conn.mget(&keys).await?;
        let mut out = HashMap::new();
        for ((source, pool), bytes) in refs.iter().zip(values.into_iter()) {
            let Some(bytes) = bytes else {
                continue;
            };
            let value: XykPoolStateValue = serde_json::from_slice(&bytes)?;
            out.insert(XykPoolStateValue::pool_key(source, pool), value);
        }
        Ok(out)
    }

    async fn fetch_clmm(&self, refs: &[(String, String)]) -> Result<HashMap<String, ClmmPoolSnapshot>> {
        if refs.is_empty() {
            return Ok(HashMap::new());
        }
        let keys: Vec<String> = refs
            .iter()
            .map(|(source, pool)| ClmmPoolSnapshot::redis_key(source, pool))
            .collect();
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let values: Vec<Option<Vec<u8>>> = conn.mget(&keys).await?;
        let mut out = HashMap::new();
        for ((source, pool), bytes) in refs.iter().zip(values.into_iter()) {
            let Some(bytes) = bytes else {
                continue;
            };
            let value: ClmmPoolSnapshot = serde_json::from_slice(&bytes)?;
            out.insert(ClmmPoolSnapshot::pool_key(source, pool), value);
        }
        Ok(out)
    }

    async fn set_xyk_batch(&self, values: &[XykPoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        for value in values {
            let mut value = value.clone();
            value.updated_at_ms = stamp_pool_updated_at_ms(Some(value.updated_at_ms));
            let key = XykPoolStateValue::redis_key(&value.source, &value.pool_address);
            let bytes = serde_json::to_vec(&value)?;
            conn.set_ex::<_, _, ()>(key, bytes, self.ttl_secs).await?;
        }
        Ok(())
    }

    async fn set_clmm_batch(&self, pools: &[ClmmPoolSnapshot]) -> Result<()> {
        if pools.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        for pool in pools {
            if !should_publish_clmm_to_redis(pool) {
                continue;
            }
            let key = ClmmPoolSnapshot::redis_key(&pool.source, &pool.pool_address);
            let bytes = serde_json::to_vec(pool)?;
            conn.set_ex::<_, _, ()>(key, bytes, self.ttl_secs).await?;
        }
        Ok(())
    }

    async fn set_stable_batch(&self, values: &[StablePoolStateValue]) -> Result<()> {
        if values.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        for value in values {
            let mut value = value.clone();
            value.updated_at_ms = stamp_pool_updated_at_ms(Some(value.updated_at_ms));
            let key = StablePoolStateValue::redis_key(&value.source, &value.pool_address);
            let bytes = serde_json::to_vec(&value)?;
            conn.set_ex::<_, _, ()>(key, bytes, self.ttl_secs).await?;
        }
        Ok(())
    }

    async fn fetch_stable(&self, refs: &[(String, String)]) -> Result<HashMap<String, StablePoolStateValue>> {
        if refs.is_empty() {
            return Ok(HashMap::new());
        }
        let keys: Vec<String> = refs
            .iter()
            .map(|(source, pool)| StablePoolStateValue::redis_key(source, pool))
            .collect();
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let values: Vec<Option<Vec<u8>>> = conn.mget(&keys).await?;
        let mut out = HashMap::new();
        for ((source, pool), bytes) in refs.iter().zip(values.into_iter()) {
            let Some(bytes) = bytes else {
                continue;
            };
            let value: StablePoolStateValue = serde_json::from_slice(&bytes)?;
            out.insert(StablePoolStateValue::pool_key(source, pool), value);
        }
        Ok(out)
    }

    async fn set_factories(&self, factories: &[FactoryRecord]) -> Result<()> {
        if factories.is_empty() {
            return Ok(());
        }
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let bytes = serde_json::to_vec(factories)?;
        conn.set::<_, _, ()>(FACTORIES_KEY, bytes).await?;
        Ok(())
    }

    async fn fetch_factories(&self) -> Result<Vec<FactoryRecord>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let bytes: Option<Vec<u8>> = conn.get(FACTORIES_KEY).await?;
        let Some(bytes) = bytes else {
            return Ok(Vec::new());
        };
        Ok(serde_json::from_slice(&bytes)?)
    }
}

pub fn parse_pool_state_ttl_secs_from_env() -> u64 {
    std::env::var("POOL_STATE_TTL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_POOL_STATE_TTL_SECS)
        .max(1)
}

pub fn parse_quote_hydrate_max_pools_from_env() -> usize {
    std::env::var("QUOTE_HYDRATE_MAX_POOLS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_QUOTE_HYDRATE_MAX_POOLS)
        .max(1)
}

pub fn build_pool_state_store(redis_url: &str) -> Result<RedisPoolStateStore> {
    RedisPoolStateStore::new(redis_url, parse_pool_state_ttl_secs_from_env())
}

#[cfg(test)]
mod tests {
    use {super::*, crate::ClmmCoverageSnapshot};

    #[test]
    fn clmm_writeback_requires_complete_coverage() {
        let complete = ClmmPoolSnapshot {
            source: "sushi".to_string(),
            pool_address: "p1".to_string(),
            token0: "A".to_string(),
            token1: "B".to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [0; 4],
            tick: 0,
            liquidity: 1,
            factory: "factory-1".to_string(),
            ticks: vec![],
            chunk_bitmaps: vec![],
            word_bitmaps: vec![],
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(-60),
                max_loaded_tick: Some(60),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        };
        let incomplete = ClmmPoolSnapshot {
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: false,
                min_loaded_tick: Some(-60),
                max_loaded_tick: Some(60),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
            ..complete.clone()
        };

        assert!(should_publish_clmm_to_redis(&complete));
        assert!(!should_publish_clmm_to_redis(&incomplete));
        assert!(!should_publish_clmm_to_redis(&ClmmPoolSnapshot {
            coverage: None,
            ..complete
        }));
    }

    #[test]
    fn redis_key_prefix_is_chakra() {
        assert_eq!(crate::REDIS_PREFIX, "chakra:");
        assert_eq!(XykPoolStateValue::redis_key("src", "POOL"), "chakra:pool:xyk:src:POOL");
        assert_eq!(ClmmPoolSnapshot::redis_key("src", "POOL"), "chakra:pool:clmm:src:POOL");
        assert_eq!(
            StablePoolStateValue::redis_key("src", "POOL"),
            "chakra:pool:stable:src:POOL"
        );
        assert_eq!(FACTORIES_KEY, "chakra:factories");
    }

    #[test]
    fn legacy_pool_json_defaults_factory_fields() {
        let value: XykPoolStateValue = serde_json::from_str(
            r#"{
                "source":"chakra-xyk",
                "pool_address":"POOL",
                "token_a":"A",
                "token_b":"B",
                "fee_bps":30,
                "reserve_a":100,
                "reserve_b":200,
                "updated_at_ms":0
            }"#,
        )
        .unwrap();
        assert_eq!(value.factory, "");
    }

    #[test]
    fn stable_pool_value_round_trips_json_with_defaults() {
        let value = StablePoolStateValue::new(
            "chakra-stable",
            "POOL1",
            "USDC",
            "EURC",
            200_000_000_000,
            200_000_000_000,
            100,
            4,
        );
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"a\":100"));
        let restored: StablePoolStateValue = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, value);
        let legacy: StablePoolStateValue = serde_json::from_str(
            r#"{
                "source":"chakra-stable",
                "pool_address":"POOL1",
                "token_a":"USDC",
                "token_b":"EURC",
                "balance_a":1,
                "balance_b":2,
                "a":100,
                "fee_bps":4
            }"#,
        )
        .unwrap();
        assert_eq!(legacy.factory, "");
        assert_eq!(legacy.updated_at_ms, 0);
    }

    #[test]
    fn factory_record_round_trips_json() {
        let record = FactoryRecord::new("0xXYKFACTORY", "xyk", "chakra-xyk");
        let json = serde_json::to_string(&record).unwrap();
        let restored: FactoryRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, record);
        assert_eq!(restored.address, "0xXYKFACTORY");
    }

    #[tokio::test]
    async fn memory_pool_store_xyk_round_trip() {
        let store = MemoryPoolStateStore::new();
        let value = XykPoolStateValue::new("Arc venue", "POOL1", "A", "B", 30, 100, 200);
        store.set_xyk_batch(&[value.clone()]).await.unwrap();
        let got = store.fetch_xyk(&[("Arc venue".into(), "POOL1".into())]).await.unwrap();
        assert_eq!(got.get("Arc venue:POOL1"), Some(&value));
    }

    #[tokio::test]
    async fn memory_pool_store_stable_and_factories_round_trip() {
        let store = MemoryPoolStateStore::new();
        let value = StablePoolStateValue::new(
            "chakra-stable",
            "POOL1",
            "USDC",
            "EURC",
            200_000_000_000,
            200_000_000_000,
            100,
            4,
        );
        store.set_stable_batch(&[value.clone()]).await.unwrap();
        let got = store
            .fetch_stable(&[("chakra-stable".into(), "POOL1".into())])
            .await
            .unwrap();
        assert_eq!(got.get("chakra-stable:POOL1"), Some(&value));

        let factories = vec![FactoryRecord::new("0xF", "xyk", "chakra-xyk")];
        store.set_factories(&factories).await.unwrap();
        assert_eq!(store.fetch_factories().await.unwrap(), factories);
    }
}
