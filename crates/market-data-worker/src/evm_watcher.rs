//! Arc event watcher + HTTP poll fallback + fetch pipeline for Chakra (T3.3).
//!
//! Default worker path on `feature-chakra`:
//!
//! 1. `publish_bootstrap` — snapshot + pool keys + `chakra:factories` (empty
//!    factories allowed; `/ready` stays false until ≥1 pool key).
//! 2. `eth_subscribe` `"logs"` on seed/discovery factories + known pools
//!    (public `CHAKRA_RPC_WS`, failover WS list). Inclusion is final — a log
//!    touch enqueues an immediate fetch+Redis write.
//! 3. When WS is disabled/dead: HTTP `eth_getLogs` over recent blocks every
//!    ~0.5 s with a catch-up cap (`CHAKRA_EVM_MAX_CATCHUP_BLOCKS`, analogous to
//!    `LEDGER_MAX_CATCHUP`).
//! 4. Discovery every ~600 s rebuilds topology from `CHAKRA_SEED_FACTORIES`
//!    (+ optional `CHAKRA_DISCOVERY_FACTORIES`) and republishes.
//!
//! Arc adapters are never constructed on this path.

use {
    crate::{fetch_pipeline, worker::WorkerShared},
    anyhow::{bail, Context, Result},
    dex_adapters::{
        evm_logs::{
            created_pools_from_evm_logs, event_topic0_hex, filter_subscribe_addresses, normalize_evm_address,
            touched_pools_from_evm_logs, watched_event_signatures, DecodedCreated,
        },
        evm_rpc::{validate_http_urls, validate_ws_urls, EvmLog, EvmRpcClient, LogFilter, ARC_RPC_HTTP, ARC_RPC_WS},
        pool_index::{KnownPoolIndex, PoolRef},
    },
    market_snapshot::{
        bootstrap::{publish_bootstrap, BootstrapPublish},
        decimals::{EURC, USDC_ERC20},
        pool_state_store::{build_pool_state_store, FactoryRecord, PoolStateStore},
        store::build_snapshot_store,
        ClmmPoolRefSnapshot, ClmmPoolSnapshot, MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    std::{
        collections::HashSet,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::sync::{mpsc, RwLock},
    tokio_tungstenite::tungstenite::Message,
    tracing::{debug, info, warn},
};

// ─── Factory configuration ──────────────────────────────────────────────────

/// One configured venue factory (`address:xyk|stable|clmm` tuple).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryConfig {
    pub address: String,
    pub dex_type: String,
    /// `"chakra-xyk"` etc. for seed factories; `"discovered:xyk"` etc. for
    /// discovery factories (never auto-allowlisted on the aggregator).
    pub source: String,
    pub is_seed: bool,
}

impl FactoryConfig {
    pub fn parse(tuple: &str, is_seed: bool) -> Result<Self> {
        let (address, dex_type) = tuple
            .split_once(':')
            .context("factory tuple must be address:dex_type")?;
        let dex_type = dex_type.to_ascii_lowercase();
        if !matches!(dex_type.as_str(), "xyk" | "stable" | "clmm") {
            bail!("unknown factory dex_type {dex_type:?} (expected xyk|stable|clmm)");
        }
        let source = if is_seed {
            format!("chakra-{dex_type}")
        } else {
            format!("discovered:{dex_type}")
        };
        Ok(Self {
            address: normalize_evm_address(address),
            dex_type,
            source,
            is_seed,
        })
    }
}

// ─── EVM worker config ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EvmConfig {
    pub chain_id: u64,
    pub http_urls: Vec<String>,
    pub ws_urls: Vec<String>,
    pub ws_enabled: bool,
    pub redis_url: Option<String>,
    pub mbtc_address: Option<String>,
    pub seed_factories: Vec<FactoryConfig>,
    pub discovery_factories: Vec<FactoryConfig>,
    pub poll_interval: Duration,
    pub max_catchup_blocks: u64,
    pub discovery_interval_secs: u64,
}

impl Default for EvmConfig {
    fn default() -> Self {
        Self {
            chain_id: 5042002,
            http_urls: vec![ARC_RPC_HTTP.to_string()],
            ws_urls: vec![ARC_RPC_WS.to_string()],
            ws_enabled: true,
            redis_url: None,
            mbtc_address: None,
            seed_factories: Vec::new(),
            discovery_factories: Vec::new(),
            poll_interval: Duration::from_millis(500),
            max_catchup_blocks: 64,
            discovery_interval_secs: 600,
        }
    }
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn split_list(value: Option<String>) -> Vec<String> {
    match value {
        Some(value) => value
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => Vec::new(),
    }
}

fn parse_bool_env(name: &str, default: bool) -> bool {
    env_var(name)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

impl EvmConfig {
    /// Build from `CHAKRA_*` env. Rejects Canteen `$RPC` and any URL not on
    /// the documented public Arc + failover lists.
    pub fn from_env() -> Result<Self> {
        let mut http_list = split_list(env_var("CHAKRA_RPC_HTTP").or_else(|| Some(ARC_RPC_HTTP.to_string())));
        http_list.extend(split_list(env_var("CHAKRA_RPC_HTTP_FAILOVERS")));
        let http_urls = validate_http_urls(&http_list)?;

        let mut ws_list = split_list(env_var("CHAKRA_RPC_WS").or_else(|| Some(ARC_RPC_WS.to_string())));
        ws_list.extend(split_list(env_var("CHAKRA_RPC_WS_FAILOVERS")));
        let ws_urls = validate_ws_urls(&ws_list)?;

        // CHAKRA_REDIS_URL → snapshot store; SNAPSHOT_REDIS_URL stays the legacy override.
        let redis_url = env_var("SNAPSHOT_REDIS_URL").or_else(|| env_var("CHAKRA_REDIS_URL"));

        let seed_factories = split_list(env_var("CHAKRA_SEED_FACTORIES"))
            .iter()
            .map(|tuple| FactoryConfig::parse(tuple, true))
            .collect::<Result<Vec<_>>>()?;
        let discovery_factories = split_list(env_var("CHAKRA_DISCOVERY_FACTORIES"))
            .iter()
            .map(|tuple| FactoryConfig::parse(tuple, false))
            .collect::<Result<Vec<_>>>()?;

        let chain_id = env_var("CHAKRA_CHAIN_ID")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5042002);
        let poll_ms = env_var("CHAKRA_EVM_POLL_INTERVAL_MS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(500)
            .max(100);
        let max_catchup_blocks = env_var("CHAKRA_EVM_MAX_CATCHUP_BLOCKS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(64)
            .max(1);
        let discovery_interval_secs = env_var("CHAKRA_DISCOVERY_INTERVAL_SECS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(600)
            .max(30);
        let ws_enabled = parse_bool_env("CHAKRA_EVM_WS_ENABLED", true) && !ws_urls.is_empty();

        Ok(Self {
            chain_id,
            http_urls,
            ws_urls,
            ws_enabled,
            redis_url,
            mbtc_address: env_var("CHAKRA_MBTC_ADDRESS").filter(|s| !s.is_empty()),
            seed_factories,
            discovery_factories,
            poll_interval: Duration::from_millis(poll_ms),
            max_catchup_blocks,
            discovery_interval_secs,
        })
    }

    pub fn all_factories(&self) -> impl Iterator<Item = &FactoryConfig> {
        self.seed_factories.iter().chain(self.discovery_factories.iter())
    }

    pub fn factory_for_source(&self, source: &str) -> Option<&FactoryConfig> {
        self.all_factories().find(|f| f.source == source)
    }
}

/// Catalog pairs Discovery probes each factory with. mBTC pairs are only
/// probed when an mBTC address is configured (else `getPair` would be called
/// with an empty address).
pub fn catalog_pairs(mbtc_address: &str) -> Vec<(String, String)> {
    let mut pairs = vec![(USDC_ERC20.to_string(), EURC.to_string())];
    if !mbtc_address.is_empty() {
        let mbtc = mbtc_address.to_string();
        pairs.push((USDC_ERC20.to_string(), mbtc.clone()));
        pairs.push((EURC.to_string(), mbtc));
    }
    pairs
}

/// CLMM fees probed during discovery: 3000 (30 bps) required, 500 (5 bps) optional.
pub const CLMM_DISCOVERY_FEES: &[(u32, u32, i32)] = &[(3000, 30, 60), (500, 5, 10)];

fn pair_key(a: &str, b: &str) -> (String, String) {
    let (a, b) = (normalize_evm_address(a), normalize_evm_address(b));
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

// ─── Runner ─────────────────────────────────────────────────────────────────

/// Live worker state shared between discovery, poll/watch, and tests.
pub(crate) struct EvmRunner {
    pub config: EvmConfig,
    pub http: Arc<EvmRpcClient>,
    pub shared: Arc<RwLock<WorkerShared>>,
    pub pipeline: Option<fetch_pipeline::FetchPipelineHandle>,
    index: KnownPoolIndex,
    poll_cursor: Option<u64>,
    watch: Option<Arc<RwLock<Vec<String>>>>,
    watch_revision: Arc<AtomicU64>,
}

impl EvmRunner {
    pub(crate) fn new(
        config: EvmConfig,
        http: EvmRpcClient,
        shared: Arc<RwLock<WorkerShared>>,
        pipeline: Option<fetch_pipeline::FetchPipelineHandle>,
    ) -> Self {
        Self {
            config,
            http: Arc::new(http),
            shared,
            pipeline,
            index: KnownPoolIndex::rebuild(&[], &[]),
            poll_cursor: None,
            watch: None,
            watch_revision: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Factories + known pool addresses to subscribe (never-call filtered).
    pub async fn compute_watch_addresses(&self) -> Vec<String> {
        let mut addresses: Vec<String> = self.config.all_factories().map(|f| f.address.clone()).collect();
        let guard = self.shared.read().await;
        for source in &guard.sources {
            for pair in &source.pairs {
                addresses.push(normalize_evm_address(&pair.pool_address));
            }
        }
        for pool in &guard.clmm_pools {
            addresses.push(normalize_evm_address(&pool.pool_address));
        }
        filter_subscribe_addresses(&addresses)
    }

    async fn sync_watch(&self) {
        let Some(watch) = &self.watch else {
            return;
        };
        let addresses = self.compute_watch_addresses().await;
        *watch.write().await = addresses;
        self.watch_revision.fetch_add(1, Ordering::Relaxed);
    }

    /// Rebuild topology from seed/discovery factories. Returns the number of
    /// pools found. Never a full-market sweep — `getPair`/`getPool` over the
    /// catalog pairs only.
    pub async fn discover_once(&mut self) -> Result<usize> {
        let mut sources_xyk: Vec<(String, TradingPairSnapshot)> = Vec::new();
        let mut sources_stable: Vec<(String, TradingPairSnapshot)> = Vec::new();
        let mut clmm_pools: Vec<ClmmPoolSnapshot> = Vec::new();
        let factories = self.config.all_factories().cloned().collect::<Vec<_>>();
        let pairs = catalog_pairs(self.config.mbtc_address.as_deref().unwrap_or(""));

        for factory in &factories {
            for (token_a, token_b) in &pairs {
                match factory.dex_type.as_str() {
                    "xyk" => {
                        if let Some(pool) = dex_adapters::evm_fetch::factory_has_xyk_pair(
                            &self.http,
                            &factory.address,
                            token_a,
                            token_b,
                        )
                        .await?
                        {
                            let (a, b) = pair_key(token_a, token_b);
                            sources_xyk.push((
                                factory.source.clone(),
                                TradingPairSnapshot {
                                    token_a: a,
                                    token_b: b,
                                    pool_address: pool,
                                    fee_bps: 30,
                                    dex_type: "xyk".to_string(),
                                    factory: factory.address.clone(),
                                },
                            ));
                        }
                    }
                    "stable" => {
                        if let Some(pool) = dex_adapters::evm_fetch::factory_has_stable_pool(
                            &self.http,
                            &factory.address,
                            token_a,
                            token_b,
                        )
                        .await?
                        {
                            let (a, b) = pair_key(token_a, token_b);
                            sources_stable.push((
                                factory.source.clone(),
                                TradingPairSnapshot {
                                    token_a: a,
                                    token_b: b,
                                    pool_address: pool,
                                    fee_bps: 4,
                                    dex_type: "stable".to_string(),
                                    factory: factory.address.clone(),
                                },
                            ));
                        }
                    }
                    "clmm" => {
                        for (fee, fee_bps, tick_spacing) in CLMM_DISCOVERY_FEES {
                            if let Some(pool) = dex_adapters::evm_fetch::factory_has_clmm_pool(
                                &self.http,
                                &factory.address,
                                token_a,
                                token_b,
                                *fee,
                            )
                            .await?
                            {
                                let (a, b) = pair_key(token_a, token_b);
                                clmm_pools.push(ClmmPoolSnapshot {
                                    source: factory.source.clone(),
                                    pool_address: pool,
                                    token0: a,
                                    token1: b,
                                    fee_bps: *fee_bps,
                                    tick_spacing: *tick_spacing,
                                    sqrt_price_x96: [0; 4],
                                    tick: 0,
                                    liquidity: 0,
                                    factory: factory.address.clone(),
                                    ticks: Vec::new(),
                                    chunk_bitmaps: Vec::new(),
                                    word_bitmaps: Vec::new(),
                                    coverage: None,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut sources: Vec<SourceSnapshot> = Vec::new();
        if !sources_xyk.is_empty() {
            let mut group: std::collections::BTreeMap<String, Vec<TradingPairSnapshot>> = Default::default();
            for (source, pair) in sources_xyk {
                group.entry(source).or_default().push(pair);
            }
            for (source, pairs) in group {
                sources.push(SourceSnapshot { source, pairs });
            }
        }
        if !sources_stable.is_empty() {
            let mut group: std::collections::BTreeMap<String, Vec<TradingPairSnapshot>> = Default::default();
            for (source, pair) in sources_stable {
                group.entry(source).or_default().push(pair);
            }
            for (source, pairs) in group {
                sources.push(SourceSnapshot { source, pairs });
            }
        }

        {
            let mut guard = self.shared.write().await;
            guard.sources = sources.clone();
            guard.clmm_pools = clmm_pools;
        }
        self.refresh_index().await;
        if self.watch.is_some() {
            self.sync_watch().await;
        }
        let total = sources.iter().map(|s| s.pairs.len()).sum::<usize>();
        info!(factories = factories.len(), pools = total, "Arc discovery complete");
        Ok(total)
    }

    async fn refresh_index(&mut self) {
        let (sources, clmm_pools) = {
            let guard = self.shared.read().await;
            (guard.sources.clone(), guard.clmm_pools.clone())
        };
        let clmm_refs: Vec<ClmmPoolRefSnapshot> = clmm_pools.iter().map(ClmmPoolRefSnapshot::from_pool).collect();
        self.index = KnownPoolIndex::rebuild(&sources, &clmm_refs);
    }

    /// Publish snapshot + current pool state + factories (no-RPC bootstrap).
    pub async fn publish_bootstrap(&self) -> Result<()> {
        let Some(redis_url) = &self.config.redis_url else {
            warn!("no CHAKRA_REDIS_URL — skipping bootstrap publish");
            return Ok(());
        };
        let (sources, clmm_pools) = {
            let guard = self.shared.read().await;
            (guard.sources.clone(), guard.clmm_pools.clone())
        };
        let snapshot = MarketSnapshot::from_sources(
            format!("snapshot-{}", now_ms()),
            now_ms(),
            "arc-testnet",
            sources.clone(),
        )
        .with_clmm_pool_refs(
            clmm_pools
                .iter()
                .map(ClmmPoolRefSnapshot::from_pool)
                .collect::<Vec<_>>(),
        );
        let factories = self
            .config
            .all_factories()
            .map(|f| FactoryRecord::new(&f.address, &f.dex_type, &f.source))
            .collect::<Vec<_>>();
        let publish = BootstrapPublish {
            snapshot,
            xyk_pools: Vec::new(),
            stable_pools: Vec::new(),
            clmm_pools: clmm_pools.clone(),
            factories,
        };
        publish_bootstrap(redis_url, 86_400, &publish).await?;
        let pool_count =
            self.shared.read().await.clmm_pools.len() + sources.iter().map(|s| s.pairs.len()).sum::<usize>();
        info!(redis = %redis_url, pools = pool_count, "Arc bootstrap published");
        Ok(())
    }

    /// One poll iteration: `eth_blockNumber` → `eth_getLogs` over the new
    /// window (catch-up capped) → ingest. Returns the number of pools touched.
    pub async fn poll_once(&mut self) -> Result<usize> {
        let watch = self.compute_watch_addresses().await;
        if watch.is_empty() {
            let latest = self.http.eth_block_number().await?;
            self.poll_cursor = Some(latest);
            return Ok(0);
        }
        let latest = self.http.eth_block_number().await?;
        let from_block = match self.poll_cursor {
            Some(cursor) if cursor < latest => (cursor + 1).max(latest.saturating_sub(self.config.max_catchup_blocks)),
            Some(_) => return Ok(0),
            None => latest.saturating_sub(1),
        };
        let filter = LogFilter {
            from_block: Some(from_block),
            to_block: Some(latest),
            addresses: watch,
            topics: watched_event_signatures()
                .iter()
                .map(|sig| Some(event_topic0_hex(sig)))
                .collect(),
        };
        let logs = self.http.eth_get_logs(&filter).await?;
        self.poll_cursor = Some(latest);
        if logs.is_empty() {
            return Ok(0);
        }
        Ok(self.ingest_logs(logs).await)
    }

    /// Decode logs → upsert created pools → enqueue touched-pool fetches.
    /// Returns the number of distinct pools touched.
    pub async fn ingest_logs(&mut self, logs: Vec<EvmLog>) -> usize {
        let mut changed = false;
        for created in created_pools_from_evm_logs(&logs) {
            match self.upsert_created_pool(created).await {
                Ok(true) => changed = true,
                Ok(false) => {}
                Err(error) => warn!("created-pool upsert failed: {error}"),
            }
        }
        if changed {
            self.refresh_index().await;
            if self.watch.is_some() {
                self.sync_watch().await;
            }
        }
        let touched = touched_pools_from_evm_logs(&logs, &self.index);
        if !touched.is_empty() {
            self.enqueue_touches(touched.clone());
        }
        touched.len()
    }

    fn enqueue_touches(&self, touched: HashSet<PoolRef>) {
        let Some(pipeline) = &self.pipeline else {
            warn!("no fetch pipeline — touched pools not refreshed");
            return;
        };
        pipeline.enqueue_touched(touched);
    }

    /// Enqueue fetches for every pool in the topology (startup hydration).
    pub async fn enqueue_all_discovered(&self) {
        let (sources, clmm_pools) = {
            let guard = self.shared.read().await;
            (guard.sources.clone(), guard.clmm_pools.clone())
        };
        let mut touched = HashSet::new();
        for source in &sources {
            for pair in &source.pairs {
                if pair.dex_type == "clmm" {
                    continue; // needs tick coverage; never a startup sweep
                }
                touched.insert(PoolRef {
                    source: source.source.clone(),
                    pool_address: pair.pool_address.clone(),
                });
            }
        }
        for pool in &clmm_pools {
            if market_snapshot::pool_state_store::should_publish_clmm_to_redis(pool) {
                touched.insert(PoolRef {
                    source: pool.source.clone(),
                    pool_address: pool.pool_address.clone(),
                });
            }
        }
        if !touched.is_empty() {
            self.enqueue_touches(touched);
        }
    }

    /// Insert a created pool into the shared topology (snapshot pairs + CLMM
    /// state). Returns `true` when the topology changed.
    async fn upsert_created_pool(&mut self, created: DecodedCreated) -> Result<bool> {
        let source = match &created {
            DecodedCreated::Xyk { .. } => self.config.all_factories().find(|f| f.dex_type == "xyk"),
            DecodedCreated::Stable { .. } => self.config.all_factories().find(|f| f.dex_type == "stable"),
            DecodedCreated::Clmm { .. } => self.config.all_factories().find(|f| f.dex_type == "clmm"),
        }
        .map(|f| f.source.clone());
        let Some(source) = source else {
            debug!("created pool on unconfigured factory ignored");
            return Ok(false);
        };
        let factory = self
            .config
            .factory_for_source(&source)
            .map(|f| f.address.clone())
            .unwrap_or_default();
        let mut guard = self.shared.write().await;
        match created {
            DecodedCreated::Xyk { token0, token1, pool } => {
                let exists = guard
                    .sources
                    .iter()
                    .flat_map(|s| &s.pairs)
                    .any(|p| normalize_evm_address(&p.pool_address) == pool);
                if exists {
                    return Ok(false);
                }
                let (token_a, token_b) = pair_key(&token0, &token1);
                guard.sources.push(SourceSnapshot {
                    source: source.clone(),
                    pairs: vec![TradingPairSnapshot {
                        token_a,
                        token_b,
                        pool_address: pool,
                        fee_bps: 30,
                        dex_type: "xyk".to_string(),
                        factory,
                    }],
                });
                info!(source = %source, "discovered xyk pair via log");
            }
            DecodedCreated::Stable { token0, token1, pool } => {
                let exists = guard
                    .sources
                    .iter()
                    .flat_map(|s| &s.pairs)
                    .any(|p| normalize_evm_address(&p.pool_address) == pool);
                if exists {
                    return Ok(false);
                }
                let (token_a, token_b) = pair_key(&token0, &token1);
                guard.sources.push(SourceSnapshot {
                    source: source.clone(),
                    pairs: vec![TradingPairSnapshot {
                        token_a,
                        token_b,
                        pool_address: pool,
                        fee_bps: 4,
                        dex_type: "stable".to_string(),
                        factory,
                    }],
                });
                info!(source = %source, "discovered stable pool via log");
            }
            DecodedCreated::Clmm {
                token0,
                token1,
                fee,
                tick_spacing,
                pool,
            } => {
                let exists = guard
                    .clmm_pools
                    .iter()
                    .any(|p| normalize_evm_address(&p.pool_address) == pool);
                if exists {
                    return Ok(false);
                }
                let (token0, token1) = pair_key(&token0, &token1);
                guard.clmm_pools.push(ClmmPoolSnapshot {
                    source: source.clone(),
                    pool_address: pool,
                    token0,
                    token1,
                    fee_bps: (fee / 100) as u32,
                    tick_spacing,
                    sqrt_price_x96: [0; 4],
                    tick: 0,
                    liquidity: 0,
                    factory,
                    ticks: Vec::new(),
                    chunk_bitmaps: Vec::new(),
                    word_bitmaps: Vec::new(),
                    coverage: None,
                });
                info!(source = %source, fee, "discovered clmm pool via log");
            }
        }
        Ok(true)
    }

    /// Full Arc worker loop (bootstrap → WS → poll → discovery). Never returns.
    pub async fn run(mut self) -> Result<()> {
        if let Err(error) = self.discover_once().await {
            warn!("initial discovery failed (continuing with empty topology): {error}");
        }
        if let Err(error) = self.publish_bootstrap().await {
            warn!("bootstrap publish failed: {error}");
        }
        self.enqueue_all_discovered().await;

        let watch = Arc::new(RwLock::new(self.compute_watch_addresses().await));
        self.watch = Some(watch.clone());
        self.watch_revision.fetch_add(1, Ordering::Relaxed);

        enum ArcEvent {
            Log(EvmLog),
            Poll,
            Discovery,
        }
        let (tx, mut rx) = mpsc::channel::<ArcEvent>(1024);

        if self.config.ws_enabled {
            let ws_urls = self.config.ws_urls.clone();
            let revision = self.watch_revision.clone();
            let (ws_log_tx, mut ws_log_rx) = mpsc::channel::<EvmLog>(1024);
            tokio::spawn(ws_watch_loop(ws_urls, watch, revision, ws_log_tx));
            let forward = tx.clone();
            tokio::spawn(async move {
                while let Some(log) = ws_log_rx.recv().await {
                    if forward.send(ArcEvent::Log(log)).await.is_err() {
                        break;
                    }
                }
            });
        }

        let poll_tx = tx.clone();
        let poll_interval = self.config.poll_interval;
        tokio::spawn(async move {
            let mut poll = tokio::time::interval(poll_interval);
            poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            poll.tick().await;
            loop {
                poll.tick().await;
                if poll_tx.send(ArcEvent::Poll).await.is_err() {
                    break;
                }
            }
        });
        let discovery_tx = tx.clone();
        let discovery_interval = Duration::from_secs(self.config.discovery_interval_secs);
        tokio::spawn(async move {
            let mut discovery = tokio::time::interval(discovery_interval);
            discovery.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            discovery.tick().await;
            loop {
                discovery.tick().await;
                if discovery_tx.send(ArcEvent::Discovery).await.is_err() {
                    break;
                }
            }
        });

        let pool_count = {
            let guard = self.shared.read().await;
            guard.sources.iter().map(|s| s.pairs.len()).sum::<usize>() + guard.clmm_pools.len()
        };
        info!(
            chain_id = self.config.chain_id,
            pools = pool_count,
            ws_enabled = self.config.ws_enabled,
            poll_ms = self.config.poll_interval.as_millis(),
            catchup = self.config.max_catchup_blocks,
            "Arc worker started"
        );

        loop {
            match rx.recv().await {
                Some(ArcEvent::Log(log)) => {
                    let touched = self.ingest_logs(vec![log]).await;
                    if touched > 0 {
                        debug!(touched, "WS touch ingested");
                    }
                }
                Some(ArcEvent::Poll) => {
                    if let Err(error) = self.poll_once().await {
                        warn!("Arc poll failed: {error}");
                    }
                }
                Some(ArcEvent::Discovery) => {
                    if let Err(error) = self.discover_once().await {
                        warn!("Arc discovery failed: {error}");
                    }
                    if let Err(error) = self.publish_bootstrap().await {
                        warn!("Arc discovery publish failed: {error}");
                    }
                    self.enqueue_all_discovered().await;
                }
                None => {
                    warn!("Arc event channel closed");
                    break;
                }
            }
        }
        Ok(())
    }
}

// ─── WS path ────────────────────────────────────────────────────────────────

/// Connect, subscribe to logs, forward notifications forever. Reconnects
/// across the failover list; `revision` changing (new pools discovered) forces
/// a reconnect so the address filter stays complete.
pub async fn ws_watch_loop(
    ws_urls: Vec<String>,
    watch: Arc<RwLock<Vec<String>>>,
    revision: Arc<AtomicU64>,
    log_tx: mpsc::Sender<EvmLog>,
) {
    use futures::{SinkExt, StreamExt};
    if ws_urls.is_empty() {
        return;
    }
    let mut attempt: usize = 0;
    loop {
        let url = &ws_urls[attempt % ws_urls.len()];
        // Snapshot the revision + addresses for this subscription attempt.
        let subscribed_at_rev = revision.load(Ordering::Relaxed);
        let addresses = watch.read().await.clone();
        if addresses.is_empty() {
            tokio::time::sleep(Duration::from_secs(1)).await;
            attempt += 1;
            continue;
        }
        match subscribe_once(url, &addresses).await {
            Ok(mut stream) => {
                info!(url = %url, topics = watched_event_signatures().len(), "Arc WS subscribed");
                loop {
                    if revision.load(Ordering::Relaxed) != subscribed_at_rev {
                        debug!("revision changed — reconnecting WS with fresh address filter");
                        break;
                    }
                    let message = match tokio::time::timeout(Duration::from_secs(30), stream.next()).await {
                        Ok(Some(Ok(message))) => message,
                        Ok(Some(Err(error))) => {
                            warn!("WS read error: {error}");
                            break;
                        }
                        Ok(None) | Err(_) => {
                            debug!("WS idle/closed — reconnecting");
                            break;
                        }
                    };
                    match message {
                        Message::Text(text) => {
                            if let Some(log) = parse_subscription_log(&text) {
                                if log_tx.send(log).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Message::Ping(payload) => {
                            let _ = stream.send(Message::Pong(payload)).await;
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => {
                warn!(url = %url, error = %error, "WS connect failed");
                tokio::time::sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
            }
        }
        attempt += 1;
    }
}

async fn subscribe_once(
    url: &str,
    addresses: &[String],
) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
    use futures::SinkExt;
    let (mut stream, _) = tokio_tungstenite::connect_async(url).await.context("ws connect")?;
    let topics: Vec<String> = watched_event_signatures()
        .iter()
        .map(|sig| event_topic0_hex(sig))
        .collect();
    let subscribe = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_subscribe",
        "params": ["logs", {"address": addresses, "topics": topics}]
    });
    stream
        .send(Message::Text(subscribe.to_string().into()))
        .await
        .context("send eth_subscribe")?;
    Ok(stream)
}

/// Parse an `eth_subscription` notification into its log (or `None`).
pub fn parse_subscription_log(text: &str) -> Option<EvmLog> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if value.get("method")?.as_str()? != "eth_subscription" {
        return None;
    }
    let result = value.get("params")?.get("result")?;
    EvmLog::from_json(result).ok()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ─── Entry points ───────────────────────────────────────────────────────────

/// Spawn the one-and-only fetch pipeline for the Arc path (EVM client; Arc
/// adapters are never constructed — the stub `ArcRpc` is never contacted).
pub(crate) fn spawn_arc_pipeline(
    pool_store: Arc<dyn PoolStateStore>,
    shared: Arc<RwLock<WorkerShared>>,
    http: Arc<EvmRpcClient>,
    worker_config: &crate::worker::WorkerConfig,
) -> fetch_pipeline::FetchPipelineHandle {
    let pipeline_config = fetch_pipeline::FetchPipelineConfig::from_env(worker_config.pool_state_refresh_concurrency);
    let stub = || {
        Arc::new(dex_adapters::rpc::ArcRpc::new(
            "https://Arc-rpc.invalid",
            "arc-stub",
        ))
    };
    fetch_pipeline::spawn_fetch_pipeline(
        pipeline_config,
        pool_store,
        stub(),
        Some(http),
        shared,
        Arc::new(dex_adapters::Arc venue::Arc venueAdapter::new(stub())),
        Arc::new(dex_adapters::Arc venue::Arc venueAdapter::new(stub())),
        Arc::new(dex_adapters::Arc venue::Arc venueAdapter::new(stub())),
        Arc::new(dex_adapters::Arc venue::Arc venueAdapter::new(stub())),
        Arc::new(dex_adapters::sushi::SushiAdapter::new(stub())),
        Arc::new(dex_adapters::Arc venue_clmm::Arc venueClmmAdapter::new(stub())),
        None,
    )
}

/// Top-level Arc entry. `WorkerConfig` supplies store URLs (`CHAKRA_REDIS_URL`
/// → snapshot store; `SNAPSHOT_REDIS_*` keep working as overrides).
pub(crate) async fn run_arc(config: crate::worker::WorkerConfig) -> Result<()> {
    let evm = EvmConfig::from_env()?;
    let _snapshot_store = match &config.snapshot_store {
        Some(store) => store.clone(),
        None => build_snapshot_store(
            config.snapshot_backend,
            Some(config.snapshot_dir.clone()),
            config.snapshot_redis_url.as_deref(),
            Some(config.snapshot_redis_channel.as_str()),
            Some(config.snapshot_redis_keep_latest),
        )?,
    };
    let pool_store: Option<Arc<dyn PoolStateStore>> = match &config.pool_store {
        Some(store) => Some(store.clone()),
        None => evm
            .redis_url
            .as_deref()
            .map(build_pool_state_store)
            .transpose()?
            .map(|store| Arc::new(store) as Arc<dyn PoolStateStore>),
    };
    let shared = Arc::new(RwLock::new(WorkerShared {
        sources: Vec::new(),
        clmm_pools: Vec::new(),
    }));
    let http = EvmRpcClient::new(evm.http_urls.clone())?;
    let pipeline = pool_store
        .clone()
        .map(|store| spawn_arc_pipeline(store, shared.clone(), Arc::new(http.clone()), &config));
    let runner = EvmRunner::new(evm, http, shared.clone(), pipeline);
    runner.run().await
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::worker::WorkerConfig,
        market_snapshot::{
            pool_state_store::{MemoryPoolStateStore, XykPoolStateValue},
            store::SnapshotStoreBackend,
        },
        serde_json::json,
        std::sync::{
            atomic::{AtomicU64, Ordering as AtomicOrdering},
            Mutex, OnceLock,
        },
        std::time::Instant,
        tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::{TcpListener, TcpStream},
        },
    };

    const USDC: &str = "0x3600000000000000000000000000000000000000";
    const EURC: &str = "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a";
    const MBTC: &str = "0x1111111111111111111111111111111111111111";
    const POOL: &str = "0x2222222222222222222222222222222222222222";
    const XYK_FACTORY: &str = "0x3333333333333333333333333333333333333333";

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    // ─── Fixture JSON-RPC server (same shape as dex-adapters) ─────────

    pub(crate) fn spawn_fixture_rpc(
        handler: impl Fn(&str, &serde_json::Value) -> Result<serde_json::Value, serde_json::Value> + Send + Sync + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        let handler = std::sync::Arc::new(handler);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let thread_handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("fixture runtime");
            runtime.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let _ = tx.send(format!("http://{addr}"));
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        continue;
                    };
                    let _ = handle_request(&mut socket, handler.as_ref()).await;
                }
            });
        });
        let url = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("fixture url");
        (url, thread_handle)
    }

    async fn handle_request(
        socket: &mut TcpStream,
        handler: &(dyn Fn(&str, &serde_json::Value) -> Result<serde_json::Value, serde_json::Value> + Send + Sync),
    ) -> std::io::Result<()> {
        use std::io::ErrorKind;
        let mut buf = [0u8; 8192];
        let mut filled = 0usize;
        let (content_length, body_start) = loop {
            let n = socket.read(&mut buf[filled..]).await?;
            if n == 0 {
                return Ok(());
            }
            filled += n;
            let hay = std::str::from_utf8(&buf[..filled]).unwrap_or("");
            if let Some(pos) = hay.find("\r\n\r\n") {
                let head = &hay[..pos];
                let content_length = head
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        (k.eq_ignore_ascii_case("content-length"))
                            .then(|| v.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                break (content_length, pos + 4);
            }
            if filled >= buf.len() {
                return Err(ErrorKind::InvalidData.into());
            }
        };
        if filled < body_start + content_length {
            let mut rest = vec![0u8; body_start + content_length - filled];
            socket.read_exact(&mut rest).await?;
            buf[filled..filled + rest.len()].copy_from_slice(&rest);
        }
        let body = &buf[body_start..body_start + content_length];
        let request: serde_json::Value = serde_json::from_slice(body).unwrap_or(json!({"id": 0}));
        let id = request.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = request.get("method").and_then(serde_json::Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(serde_json::Value::Null);
        let response = match handler(method, &params) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": format!("{error}")}}),
        };
        let body = serde_json::to_vec(&response).unwrap();
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await?;
        socket.write_all(&body).await?;
        socket.flush().await?;
        Ok(())
    }

    fn word(value: u128) -> String {
        format!("0x{value:0>64x}")
    }

    fn words_hex(values: &[u128]) -> String {
        let mut data = "0x".to_string();
        for value in values {
            data.push_str(&format!("{value:0>64x}"));
        }
        data
    }

    fn reserves_response(r0: u128, r1: u128) -> String {
        words_hex(&[r0, r1, 0])
    }

    fn address_topic(address: &str) -> String {
        format!("0x{:0>64}", &address[2..])
    }

    /// V2 Swap log emitted by `emitter` (sender=0x01, recipient=0x02).
    fn swap_log(emitter: &str, r0_in: u128, r1_in: u128, r0_out: u128, r1_out: u128) -> serde_json::Value {
        json!({
            "address": emitter,
            "topics": [event_topic0_hex(dex_adapters::evm_logs::XYK_SWAP_SIG), address_topic("0x0000000000000000000000000000000000000001")],
            "data": words_hex(&[r0_in, r1_in, r0_out, r1_out]),
            "blockNumber": "0x11",
            "logIndex": "0x0"
        })
    }

    fn pair_snapshot(dex_type: &str, factory: &str) -> TradingPairSnapshot {
        TradingPairSnapshot {
            token_a: USDC.to_string(),
            token_b: EURC.to_string(),
            pool_address: POOL.to_string(),
            fee_bps: if dex_type == "stable" { 4 } else { 30 },
            dex_type: dex_type.to_string(),
            factory: factory.to_string(),
        }
    }

    fn worker_config_stub() -> WorkerConfig {
        WorkerConfig {
            mode: crate::worker::WorkerMode::Arc,
            rpc_url: "https://Arc-rpc.invalid".to_string(),
            network_passphrase: "arc-testnet".to_string(),
            snapshot_backend: SnapshotStoreBackend::Memory,
            snapshot_dir: std::path::PathBuf::from("/tmp/chakra-test-snapshots"),
            snapshot_redis_url: None,
            snapshot_redis_channel: "chakra:snapshot:events".to_string(),
            snapshot_redis_keep_latest: 3,
            refresh_interval_secs: 30,
            pool_publish_interval_secs: 2,
            pool_state_refresh_concurrency: 4,
            discovery_interval_secs: 600,
            ledger_poll: std::time::Duration::from_millis(100),
            ledger_watcher_enabled: false,
            fetch_pipeline_enabled: true,
            snapshot_store: None,
            pool_store: None,
        }
    }

    // ─── Config tests ────────────────────────────────────────────────

    #[test]
    fn factory_tuple_parse_accepts_seed_and_discovery() {
        let seed = FactoryConfig::parse("0xABCD:xyk", true).unwrap();
        assert_eq!(seed.source, "chakra-xyk");
        assert_eq!(seed.address, "0x000000000000000000000000000000000000abcd");
        assert!(seed.is_seed);
        let discovered = FactoryConfig::parse("0xABCD:stable", false).unwrap();
        assert_eq!(discovered.source, "discovered:stable");
        assert!(!discovered.is_seed);
        assert!(FactoryConfig::parse("0xABCD:clmm", true).is_ok());
        assert!(FactoryConfig::parse("0xABCD:liquidity-pool", true).is_err());
        assert!(FactoryConfig::parse("no-colon", true).is_err());
    }

    #[test]
    fn evm_config_from_env_reads_chakra_vars() {
        let _guard = env_lock().lock().unwrap();
        let original = [
            "CHAKRA_RPC_HTTP",
            "CHAKRA_RPC_WS",
            "CHAKRA_RPC_HTTP_FAILOVERS",
            "CHAKRA_RPC_WS_FAILOVERS",
            "CHAKRA_REDIS_URL",
            "SNAPSHOT_REDIS_URL",
            "CHAKRA_CHAIN_ID",
            "CHAKRA_SEED_FACTORIES",
            "CHAKRA_DISCOVERY_FACTORIES",
            "CHAKRA_MBTC_ADDRESS",
            "CHAKRA_EVM_POLL_INTERVAL_MS",
        ]
        .map(|name| (name, std::env::var(name).ok()));
        for (name, _) in &original {
            std::env::remove_var(name);
        }
        std::env::set_var("CHAKRA_RPC_HTTP", "https://rpc.testnet.arc.io");
        std::env::set_var("CHAKRA_RPC_WS", "wss://rpc.testnet.arc.io");
        std::env::set_var("CHAKRA_RPC_HTTP_FAILOVERS", "https://rpc.drpc.testnet.arc.io");
        std::env::set_var("CHAKRA_RPC_WS_FAILOVERS", "wss://rpc.quicknode.testnet.arc.io");
        std::env::set_var("CHAKRA_REDIS_URL", "redis://127.0.0.1:6379/");
        std::env::set_var("CHAKRA_CHAIN_ID", "5042002");
        std::env::set_var("CHAKRA_SEED_FACTORIES", "0xAAA:xyk,0xBBB:stable");
        std::env::set_var("CHAKRA_DISCOVERY_FACTORIES", "0xCCC:clmm");
        std::env::set_var("CHAKRA_MBTC_ADDRESS", MBTC);
        std::env::set_var("CHAKRA_EVM_POLL_INTERVAL_MS", "250");

        let config = EvmConfig::from_env().unwrap();
        assert_eq!(config.chain_id, 5042002);
        assert_eq!(config.http_urls.len(), 2);
        assert!(config.ws_enabled);
        assert_eq!(config.redis_url.as_deref(), Some("redis://127.0.0.1:6379/"));
        assert_eq!(config.seed_factories.len(), 2);
        assert_eq!(config.seed_factories[0].source, "chakra-xyk");
        assert_eq!(config.discovery_factories[0].source, "discovered:clmm");
        assert_eq!(config.mbtc_address.as_deref(), Some(MBTC));
        assert_eq!(config.poll_interval.as_millis(), 250);

        for (name, value) in original {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    #[test]
    fn evm_config_rejects_canteen_rpc() {
        let _guard = env_lock().lock().unwrap();
        let original = ["CHAKRA_RPC_HTTP", "CHAKRA_RPC_WS"].map(|name| (name, std::env::var(name).ok()));
        for (name, _) in &original {
            std::env::remove_var(name);
        }
        std::env::set_var("CHAKRA_RPC_HTTP", "https://rpc.testnet.arc-node.thecanteenapp.com");
        assert!(EvmConfig::from_env().is_err(), "Canteen $RPC must be rejected");
        for (name, value) in original {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    #[test]
    fn evm_config_ws_disabled_when_disabled_by_env_or_no_urls() {
        let _guard = env_lock().lock().unwrap();
        let original =
            ["CHAKRA_RPC_HTTP", "CHAKRA_RPC_WS", "CHAKRA_EVM_WS_ENABLED"].map(|name| (name, std::env::var(name).ok()));
        for (name, _) in &original {
            std::env::remove_var(name);
        }
        std::env::set_var("CHAKRA_EVM_WS_ENABLED", "false");
        let config = EvmConfig::from_env().unwrap();
        assert!(!config.ws_enabled);
        for (name, value) in original {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    // ─── Discovery ───────────────────────────────────────────────────

    #[tokio::test]
    async fn discovery_finds_catalog_xyk_pair_from_fixture_factory() {
        let pool = POOL.to_ascii_lowercase();
        let usdc_word = format!("{:0>64}", &USDC[2..]);
        let eurc_word = format!("{:0>64}", &EURC[2..]);
        let (url, _server) = spawn_fixture_rpc(move |method, params| {
            assert_eq!(method, "eth_call");
            assert_eq!(params[0]["to"], XYK_FACTORY);
            let data = params[0]["data"].as_str().unwrap();
            if data.contains(&usdc_word) && data.contains(&eurc_word) {
                // getPair(USDC, EURC) → the seeded pool.
                return Ok(json!(format!("0x{:0>64}", &pool[2..])));
            }
            Ok(json!(word(0)))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            mbtc_address: Some(MBTC.to_string()),
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared.clone(), None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 1);
        let guard = shared.read().await;
        assert_eq!(guard.sources.len(), 1);
        assert_eq!(guard.sources[0].pairs.len(), 1);
        let pair = &guard.sources[0].pairs[0];
        assert_eq!(pair.pool_address, POOL.to_ascii_lowercase());
        assert_eq!(pair.dex_type, "xyk");
        assert_eq!(pair.fee_bps, 30);
        assert_eq!(pair.factory, XYK_FACTORY);
        let watch = runner.compute_watch_addresses().await;
        assert!(watch.contains(&XYK_FACTORY.to_ascii_lowercase()));
        assert!(watch.contains(&POOL.to_ascii_lowercase()));
    }

    // ─── Poll path (SC-11 local) ─────────────────────────────────────

    #[tokio::test]
    async fn poll_refreshes_pool_store_after_fixture_swap_within_5s() {
        let swap = swap_log(POOL, 1_000_000, 1_000_000, 90_000_000, 110_000_000);
        let block = Arc::new(AtomicU64::new(0x10));
        let block_inside = block.clone();
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_blockNumber" => {
                let b = block_inside.load(AtomicOrdering::Relaxed);
                Ok(json!(format!("0x{b:x}")))
            }
            "eth_getLogs" => {
                let from = params[0]["fromBlock"].as_str().unwrap().to_string();
                let to = params[0]["toBlock"].as_str().unwrap().to_string();
                if to == "0x11" {
                    Ok(json!([swap.clone()]))
                } else {
                    debug_assert_eq!(from, "0xf");
                    Ok(json!([]))
                }
            }
            "eth_call" => Ok(json!(reserves_response(90_000_000, 110_000_000))),
            other => Err(json!(format!("unexpected method {other}"))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            mbtc_address: Some(MBTC.to_string()),
            ws_enabled: false,
            poll_interval: std::time::Duration::from_millis(200),
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: vec![SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![pair_snapshot("xyk", XYK_FACTORY)],
            }],
            clmm_pools: Vec::new(),
        }));
        let pool_store: Arc<MemoryPoolStateStore> = Arc::new(MemoryPoolStateStore::new());
        pool_store
            .set_xyk_batch(&[XykPoolStateValue::new(
                "chakra-xyk",
                POOL,
                USDC,
                EURC,
                30,
                100_000_000,
                100_000_000,
            )])
            .await
            .unwrap();

        let pipeline = Some(spawn_arc_pipeline(
            pool_store.clone(),
            shared.clone(),
            Arc::new(client.clone()),
            &worker_config_stub(),
        ));
        let mut runner = EvmRunner::new(config, client, shared.clone(), pipeline);
        runner.refresh_index().await;

        let start = Instant::now();
        // First poll: cursor init (block 0x10, no logs in 15..16).
        assert_eq!(runner.poll_once().await.unwrap(), 0);
        // Chain advances; second poll sees the swap in block 0x11.
        block.store(0x11, AtomicOrdering::Relaxed);
        assert_eq!(runner.poll_once().await.unwrap(), 1);

        // Pipeline writes asynchronously — wait (≤ 5 s per SC-11).
        let updated = loop {
            let state = pool_store
                .fetch_xyk(&[("chakra-xyk".into(), POOL.to_string())])
                .await
                .unwrap();
            if let Some(value) = state.get(&format!("chakra-xyk:{POOL}")) {
                if value.reserve_a == 90_000_000 && value.reserve_b == 110_000_000 {
                    break Some((value.reserve_a, value.reserve_b));
                }
            }
            if start.elapsed() >= std::time::Duration::from_secs(5) {
                break None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        let elapsed = start.elapsed();
        assert_eq!(updated, Some((90_000_000, 110_000_000)));
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "fixture swap → store write took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn poll_with_empty_topology_keeps_cursor_warm_without_logs() {
        let (url, _server) = spawn_fixture_rpc(|method, _| match method {
            "eth_blockNumber" => Ok(json!("0x10")),
            other => Err(json!(format!("unexpected {other}"))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(EvmConfig::default(), client, shared, None);
        // No factories/pools → no eth_getLogs call, only block number.
        assert_eq!(runner.poll_once().await.unwrap(), 0);
        assert_eq!(runner.poll_cursor, Some(0x10));
    }

    // ─── Created pools ───────────────────────────────────────────────

    #[tokio::test]
    async fn created_pool_log_upserts_topology_and_later_swap_touches() {
        let new_pool = "0x4444444444444444444444444444444444444444";
        // ABI word: 12 zero bytes + 20-byte pair address; then allPairsLength.
        let pool_word = format!("{:0>64}", &new_pool[2..]);
        let created = json!({
            "address": XYK_FACTORY,
            "topics": [
                event_topic0_hex(dex_adapters::evm_logs::XYK_PAIR_CREATED_SIG),
                address_topic(USDC),
                address_topic(EURC)
            ],
            "data": format!("0x{pool_word}{:0>64x}", 1u128),
            "blockNumber": "0x11",
            "logIndex": "0x0"
        });
        let swap = swap_log(new_pool, 0, 0, 1, 1);
        let logs: Vec<EvmLog> = [created, swap]
            .iter()
            .map(|value| EvmLog::from_json(value).unwrap())
            .collect();

        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(
            config,
            EvmRpcClient::single("http://127.0.0.1:9").unwrap(),
            shared.clone(),
            None,
        );

        let touched = runner.ingest_logs(logs).await;
        assert_eq!(touched, 1);

        let guard = shared.read().await;
        assert_eq!(guard.sources.len(), 1);
        let pair = &guard.sources[0].pairs[0];
        assert_eq!(pair.pool_address, new_pool);
        assert_eq!(pair.dex_type, "xyk");
        // Touched pool resolves through the refreshed index.
        let pool = runner.index.lookup_contract(new_pool).unwrap().clone();
        assert_eq!(pool.source, "chakra-xyk");
    }

    // ─── WS path ─────────────────────────────────────────────────────

    #[test]
    fn parse_subscription_log_extracts_log() {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "eth_subscription",
            "params": {"subscription": "0xsub1", "result": swap_log(POOL, 1, 2, 3, 4)}
        })
        .to_string();
        let log = parse_subscription_log(&notification).unwrap();
        assert_eq!(log.address, POOL);
        assert!(dex_adapters::evm_logs::is_pool_touch_topic(&log.topics[0]));
        assert!(parse_subscription_log("{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"0x1\"}").is_none());
    }

    #[tokio::test]
    async fn ws_subscription_forwards_log_notification() {
        use futures::{SinkExt, StreamExt};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let swap = swap_log(POOL, 5, 6, 7, 8);
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            loop {
                match ws.next().await {
                    Some(Ok(Message::Text(text))) => {
                        let request: serde_json::Value = serde_json::from_str(&text).unwrap();
                        assert_eq!(request["method"], "eth_subscribe");
                        let id = request["id"].clone();
                        ws.send(Message::Text(
                            json!({"jsonrpc": "2.0", "id": id, "result": "0xsub1"})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                        ws.send(Message::Text(
                            json!({
                                "jsonrpc": "2.0",
                                "method": "eth_subscription",
                                "params": {"subscription": "0xsub1", "result": swap}
                            })
                            .to_string()
                            .into(),
                        ))
                        .await
                        .unwrap();
                        break;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        ws.send(Message::Pong(payload)).await.unwrap();
                    }
                    _ => {}
                }
            }
        });

        let watch = Arc::new(RwLock::new(vec![POOL.to_string()]));
        let revision = Arc::new(AtomicU64::new(0));
        let (tx, mut rx) = mpsc::channel::<EvmLog>(16);
        let loop_handle = tokio::spawn(ws_watch_loop(vec![format!("ws://{addr}")], watch, revision, tx));

        let log = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out waiting for WS log")
            .unwrap();
        assert_eq!(log.address, POOL);
        assert!(dex_adapters::evm_logs::is_pool_touch_topic(&log.topics[0]));

        loop_handle.abort();
        server.abort();
    }

    #[test]
    fn watch_addresses_filter_never_call_and_keep_0x() {
        let config = EvmConfig {
            seed_factories: vec![
                FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap(),
                FactoryConfig::parse("0x8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA:xyk", true).unwrap(), // CCTP TokenMessengerV2 — must be dropped
            ],
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let tokio_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let addresses = tokio_rt.block_on(async {
            let runner = EvmRunner::new(
                config,
                EvmRpcClient::single("http://127.0.0.1:9").unwrap(),
                shared,
                None,
            );
            runner.compute_watch_addresses().await
        });
        assert!(addresses.contains(&XYK_FACTORY.to_ascii_lowercase()));
        assert!(!addresses
            .iter()
            .any(|a| a == "0x8fe6b999dc680ccfdd5bf7eb0974218be2542daa"));
        assert_eq!(addresses.len(), 1);
    }

    /// Sanity: discovery does not touch mBTC pairs when mBTC is unset.
    #[tokio::test]
    async fn discovery_without_mbtc_only_probes_usdc_eurc() {
        let (url, _server) = spawn_fixture_rpc(|_method, _params| Ok(json!(word(0))));
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        // mbtc_address = None → catalog pairs collapse to USDC/EURC only.
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        assert_eq!(runner.discover_once().await.unwrap(), 0);
    }
}
