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
        if !matches!(dex_type.as_str(), "xyk" | "stable" | "clmm" | "xylo" | "presto") {
            bail!("unknown factory dex_type {dex_type:?} (expected xyk|stable|clmm|xylo|presto)");
        }
        // 2026-08-29 canonical source ids: xylo → xylo-stable, presto → presto-hub;
        // seeded unitflow/xyk factories → unitflow-v25 / chakra-xyk.
        const UNITFLOW_FACTORY_ADDR: &str = "0xd67f63a4f26a497b364d1c82e6747aec8b5743a5";
        let normalized = normalize_evm_address(address);
        let source = match dex_type.as_str() {
            "xylo" if is_seed => "xylo-stable".to_string(),
            "presto" if is_seed => "presto-hub".to_string(),
            "xyk" if is_seed && normalized == UNITFLOW_FACTORY_ADDR => "unitflow-v25".to_string(),
            "xyk" if is_seed => "chakra-xyk".to_string(),
            "stable" if is_seed => "chakra-stable".to_string(),
            "clmm" if is_seed => "chakra-clmm".to_string(),
            _ => format!("discovered:{dex_type}"),
        };
        Ok(Self {
            address: normalized,
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

        let redis_url = env_var("CHAKRA_REDIS_URL");

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

/// Catalog pairs Discovery probes each factory with. cirBTC pairs are always
/// probed (canonical address, 2026-08-29).
pub fn catalog_pairs() -> Vec<(String, String)> {
    vec![
        (USDC_ERC20.to_string(), EURC.to_string()),
        (USDC_ERC20.to_string(), market_snapshot::decimals::CIRBTC.to_string()),
        (EURC.to_string(), market_snapshot::decimals::CIRBTC.to_string()),
    ]
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
    /// Manifest venues that passed startup verification (bytecode present).
    /// Only these are published to `chakra:factories` (SC-15 2026-08-29).
    verified_factories: std::sync::RwLock<Vec<FactoryConfig>>,
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
            verified_factories: std::sync::RwLock::new(Vec::new()),
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

    async fn address_has_code(&self, address: &str) -> bool {
        matches!(
            self.http.eth_get_code(address).await,
            Ok(code) if code.len() > 2 && code[2..].bytes().any(|b| b != b'0')
        )
    }

    /// Rebuild topology from seed/discovery factories. Returns the number of
    /// pools found. Never a full-market sweep — `getPair`/`getPool` over the
    /// catalog pairs only.
    pub async fn discover_once(&mut self) -> Result<usize> {
        let mut sources_xyk: Vec<(String, TradingPairSnapshot)> = Vec::new();
        let mut sources_stable: Vec<(String, TradingPairSnapshot)> = Vec::new();
        let mut clmm_pools: Vec<ClmmPoolSnapshot> = Vec::new();
        let factories = self.config.all_factories().cloned().collect::<Vec<_>>();
        let pairs = catalog_pairs();
        let mut verified: Vec<FactoryConfig> = Vec::new();

        for factory in &factories {
            // 2026-08-29 manifest venue verification (SC-15): a seed factory
            // must pass all 5 checks: bytecode, canonical endpoints, factory membership,
            // nonzero reserves, and probe quote. Unavailable venues yield NO_ROUTE;
            // Chakra never automatically reseeds them.
            if factory.is_seed {
                if !self.address_has_code(&factory.address).await {
                    warn!(
                        address = %factory.address,
                        source = %factory.source,
                        "manifest venue has no bytecode — marked unavailable"
                    );
                    continue;
                }
            } else {
                // Discovery-only factories are never auto-enabled; keep them
                // in the topology/discovery set (owner `addFactory` gates quotes).
                verified.push(factory.clone());
            }

            let initial_pools = sources_xyk.len() + sources_stable.len() + clmm_pools.len();
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
                            // T3.3 / SC-15 five-check verification:
                            // 1. Bytecode: pool address has deployed code
                            if !self.address_has_code(&pool).await {
                                warn!(pool = %pool, "xyk pool has no bytecode — skipped");
                                continue;
                            }
                            // 2. Canonical token endpoints: pool token0/token1 match catalog tokens
                            if !dex_adapters::evm_fetch::verify_canonical_token_endpoints(
                                &self.http, "xyk", &pool, token_a, token_b,
                            )
                            .await
                            .unwrap_or(false)
                            {
                                warn!(pool = %pool, "xyk pool token endpoints mismatch — skipped");
                                continue;
                            }
                            let (a, b) = pair_key(token_a, token_b);
                            let snapshot = TradingPairSnapshot {
                                token_a: a.clone(),
                                token_b: b.clone(),
                                pool_address: pool.clone(),
                                fee_bps: 30,
                                dex_type: "xyk".to_string(),
                                factory: factory.address.clone(),
                            };
                            // 4 & 5. Nonzero reserves & probe quote
                            let Ok(state) =
                                dex_adapters::evm_fetch::fetch_xyk_state(&self.http, &factory.source, &snapshot).await
                            else {
                                warn!(pool = %pool, "xyk pool state fetch failed — skipped");
                                continue;
                            };
                            if state.reserve_a == 0 || state.reserve_b == 0 {
                                warn!(pool = %pool, "xyk pool has zero reserves — skipped");
                                continue;
                            }
                            if dex_adapters::evm_quote_math::xyk_quote(state.reserve_a, state.reserve_b, 1_000_000) == 0
                            {
                                warn!(pool = %pool, "xyk pool probe quote failed — skipped");
                                continue;
                            }
                            sources_xyk.push((factory.source.clone(), snapshot));
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
                            if !self.address_has_code(&pool).await {
                                warn!(pool = %pool, "stable pool has no bytecode — skipped");
                                continue;
                            }
                            if !dex_adapters::evm_fetch::verify_canonical_token_endpoints(
                                &self.http, "stable", &pool, token_a, token_b,
                            )
                            .await
                            .unwrap_or(false)
                            {
                                warn!(pool = %pool, "stable pool token endpoints mismatch — skipped");
                                continue;
                            }
                            let (a, b) = pair_key(token_a, token_b);
                            let snapshot = TradingPairSnapshot {
                                token_a: a.clone(),
                                token_b: b.clone(),
                                pool_address: pool.clone(),
                                fee_bps: 4,
                                dex_type: "stable".to_string(),
                                factory: factory.address.clone(),
                            };
                            let Ok(state) = dex_adapters::evm_fetch::fetch_stable_state(
                                &self.http,
                                &factory.source,
                                &snapshot,
                                dex_adapters::evm_fetch::CHAKRA_STABLE_A,
                            )
                            .await
                            else {
                                warn!(pool = %pool, "stable pool state fetch failed — skipped");
                                continue;
                            };
                            if state.balance_a == 0 || state.balance_b == 0 {
                                warn!(pool = %pool, "stable pool has zero balances — skipped");
                                continue;
                            }
                            if dex_adapters::evm_quote_math::stable_quote(&state, 0, 1, 1_000_000) == 0 {
                                warn!(pool = %pool, "stable pool probe quote failed — skipped");
                                continue;
                            }
                            sources_stable.push((factory.source.clone(), snapshot));
                        }
                    }
                    "xylo" => {
                        if let Some(pool) = dex_adapters::evm_fetch::factory_has_stable_pool(
                            &self.http,
                            &factory.address,
                            token_a,
                            token_b,
                        )
                        .await?
                        {
                            if !self.address_has_code(&pool).await {
                                warn!(pool = %pool, "xylo pool has no bytecode — skipped");
                                continue;
                            }
                            if !dex_adapters::evm_fetch::verify_canonical_token_endpoints(
                                &self.http, "xylo", &pool, token_a, token_b,
                            )
                            .await
                            .unwrap_or(false)
                            {
                                warn!(pool = %pool, "xylo pool token endpoints mismatch — skipped");
                                continue;
                            }
                            let (a, b) = pair_key(token_a, token_b);
                            let snapshot = TradingPairSnapshot {
                                token_a: a.clone(),
                                token_b: b.clone(),
                                pool_address: pool.clone(),
                                fee_bps: 4,
                                dex_type: "xylo".to_string(),
                                factory: factory.address.clone(),
                            };
                            let Ok(state) =
                                dex_adapters::evm_fetch::fetch_xylo_state(&self.http, &factory.source, &snapshot).await
                            else {
                                warn!(pool = %pool, "xylo pool state fetch failed — skipped");
                                continue;
                            };
                            if state.balance_a == 0 || state.balance_b == 0 {
                                warn!(pool = %pool, "xylo pool has zero reserves — skipped");
                                continue;
                            }
                            if dex_adapters::evm_quote_math::xylo_quote_with_a(
                                state.balance_a,
                                state.balance_b,
                                1_000_000,
                                state.a,
                            ) == 0
                            {
                                warn!(pool = %pool, "xylo pool probe quote failed — skipped");
                                continue;
                            }
                            sources_stable.push((factory.source.clone(), snapshot));
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
                                if !self.address_has_code(&pool).await {
                                    warn!(pool = %pool, "clmm pool has no bytecode — skipped");
                                    continue;
                                }
                                if !dex_adapters::evm_fetch::verify_canonical_token_endpoints(
                                    &self.http, "clmm", &pool, token_a, token_b,
                                )
                                .await
                                .unwrap_or(false)
                                {
                                    warn!(pool = %pool, "clmm pool token endpoints mismatch — skipped");
                                    continue;
                                }
                                let (a, b) = pair_key(token_a, token_b);
                                let pool_ref = ClmmPoolRefSnapshot {
                                    source: factory.source.clone(),
                                    pool_address: pool.clone(),
                                    token0: a.clone(),
                                    token1: b.clone(),
                                    fee_bps: *fee_bps,
                                    tick_spacing: *tick_spacing,
                                    factory: factory.address.clone(),
                                };
                                let Ok(state) = dex_adapters::evm_fetch::fetch_clmm_state(
                                    &self.http,
                                    &factory.source,
                                    &pool_ref,
                                    None,
                                )
                                .await
                                else {
                                    warn!(pool = %pool, "clmm pool state fetch failed — skipped");
                                    continue;
                                };
                                if state.liquidity == 0 || state.sqrt_price_x96 == [0; 4] {
                                    warn!(pool = %pool, "clmm pool has zero liquidity or price — skipped");
                                    continue;
                                }
                                let pool_state = dex_adapters::clmm_math::ClmmPoolState {
                                    sqrt_price_x96: dex_adapters::clmm_math::U256(state.sqrt_price_x96),
                                    tick: state.tick,
                                    liquidity: state.liquidity,
                                    fee_bps: *fee_bps,
                                    tick_spacing: *tick_spacing,
                                    token0: a.clone(),
                                    token1: b.clone(),
                                };
                                let tick_store = dex_adapters::clmm_math::TickDataStore::new();
                                let Some((amount_out, _, _)) =
                                    dex_adapters::clmm_math::simulate_swap(&pool_state, &tick_store, 1_000_000, true)
                                else {
                                    warn!(pool = %pool, "clmm pool probe quote failed — skipped");
                                    continue;
                                };
                                if amount_out == 0 {
                                    warn!(pool = %pool, "clmm pool probe quote zero — skipped");
                                    continue;
                                }
                                clmm_pools.push(state);
                            }
                        }
                    }
                    "presto" => {
                        let is_usdc_eurc = (token_a.eq_ignore_ascii_case(USDC_ERC20)
                            && token_b.eq_ignore_ascii_case(EURC))
                            || (token_a.eq_ignore_ascii_case(EURC) && token_b.eq_ignore_ascii_case(USDC_ERC20));
                        if is_usdc_eurc {
                            if !dex_adapters::evm_fetch::verify_canonical_token_endpoints(
                                &self.http,
                                "presto",
                                &factory.address,
                                token_a,
                                token_b,
                            )
                            .await
                            .unwrap_or(false)
                            {
                                warn!(hub = %factory.address, "presto hub pathUSD endpoint mismatch — skipped");
                                continue;
                            }
                            let (a, b) = pair_key(token_a, token_b);
                            let snapshot = TradingPairSnapshot {
                                token_a: a.clone(),
                                token_b: b.clone(),
                                pool_address: factory.address.clone(),
                                fee_bps: 30,
                                dex_type: "presto".to_string(),
                                factory: factory.address.clone(),
                            };
                            let Ok(state) =
                                dex_adapters::evm_fetch::fetch_presto_state(&self.http, &factory.source, &snapshot)
                                    .await
                            else {
                                warn!(hub = %factory.address, "presto hub spoke fetch failed — skipped");
                                continue;
                            };
                            if state.balance_a == 0 || state.balance_b == 0 {
                                warn!(hub = %factory.address, "presto hub has zero reserves — skipped");
                                continue;
                            }
                            if dex_adapters::evm_quote_math::presto_spoke_quote(
                                state.balance_a,
                                state.balance_b,
                                1_000_000,
                            ) == 0
                            {
                                warn!(hub = %factory.address, "presto hub probe quote failed — skipped");
                                continue;
                            }
                            sources_stable.push((factory.source.clone(), snapshot));
                        }
                    }
                    _ => {}
                }
            }

            let final_pools = sources_xyk.len() + sources_stable.len() + clmm_pools.len();
            if factory.is_seed && final_pools > initial_pools {
                verified.push(factory.clone());
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
        *self.verified_factories.write().unwrap() = verified;
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
        // 2026-08-29 (SC-15): only verified manifest venues are published to
        // `chakra:factories` — a venue that failed startup verification stays
        // unavailable and yields NO_ROUTE (never auto-reseeded).
        let factories = self
            .verified_factories
            .read()
            .unwrap()
            .iter()
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
                    fee_bps: fee / 100,
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
        .send(Message::Text(subscribe.to_string()))
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

/// Spawn the fetch pipeline for Arc EVM venues.
pub(crate) fn spawn_arc_pipeline(
    pool_store: Arc<dyn PoolStateStore>,
    shared: Arc<RwLock<WorkerShared>>,
    http: Arc<EvmRpcClient>,
    worker_config: &crate::worker::WorkerConfig,
) -> fetch_pipeline::FetchPipelineHandle {
    let pipeline_config = fetch_pipeline::FetchPipelineConfig::from_env(worker_config.pool_state_refresh_concurrency);
    fetch_pipeline::spawn_fetch_pipeline(pipeline_config, pool_store, http, shared)
}

/// Top-level Arc entry. `WorkerConfig` supplies the active Chakra store URL.
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
pub(crate) mod tests {
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
    const POOL: &str = "0x2222222222222222222222222222222222222222";
    const XYK_FACTORY: &str = "0x3333333333333333333333333333333333333333";
    const PRESTO_HUB: &str = "0x5794a8284A29493871Fbfa3c4f343D42001424D6";

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
            snapshot_backend: SnapshotStoreBackend::Memory,
            snapshot_dir: std::path::PathBuf::from("/tmp/chakra-test-snapshots"),
            snapshot_redis_url: None,
            snapshot_redis_channel: "chakra:snapshot:events".to_string(),
            snapshot_redis_keep_latest: 3,
            pool_state_refresh_concurrency: 4,
            discovery_interval_secs: 600,
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
        // 2026-08-29 canonical source ids for seed vs discovery.
        let xylo_seed = FactoryConfig::parse("0xABCD:xylo", true).unwrap();
        assert_eq!(xylo_seed.source, "xylo-stable");
        let xylo_disc = FactoryConfig::parse("0xABCD:xylo", false).unwrap();
        assert_eq!(xylo_disc.source, "discovered:xylo");
        let presto_seed = FactoryConfig::parse("0xABCD:presto", true).unwrap();
        assert_eq!(presto_seed.source, "presto-hub");
        let presto_disc = FactoryConfig::parse("0xABCD:presto", false).unwrap();
        assert_eq!(presto_disc.source, "discovered:presto");
        assert!(FactoryConfig::parse("0xABCD:liquidity-pool", true).is_err());
        assert!(FactoryConfig::parse("no-colon", true).is_err());
    }

    #[test]
    fn unitflow_factory_parsed_as_unitflow_v25() {
        const UNITFLOW_FACTORY: &str = "0xd67F63A4F26a497b364d1C82e6747Aec8B5743a5";
        let unitflow = FactoryConfig::parse(&format!("{UNITFLOW_FACTORY}:xyk"), true).unwrap();
        assert_eq!(unitflow.source, "unitflow-v25");
        assert_eq!(unitflow.address, UNITFLOW_FACTORY.to_ascii_lowercase());

        // Other seeded xyk factories stay chakra-xyk (e.g. 31337 fixtures).
        let fixture_xyk = FactoryConfig::parse("0x0c812E5D55D767533c8E4783D33b28EA825b4D8e:xyk", true).unwrap();
        assert_eq!(fixture_xyk.source, "chakra-xyk");
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
            "CHAKRA_CHAIN_ID",
            "CHAKRA_SEED_FACTORIES",
            "CHAKRA_DISCOVERY_FACTORIES",
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
        std::env::set_var("CHAKRA_EVM_POLL_INTERVAL_MS", "250");

        let config = EvmConfig::from_env().unwrap();
        assert_eq!(config.chain_id, 5042002);
        assert_eq!(config.http_urls.len(), 2);
        assert!(config.ws_enabled);
        assert_eq!(config.redis_url.as_deref(), Some("redis://127.0.0.1:6379/"));
        assert_eq!(config.seed_factories.len(), 2);
        assert_eq!(config.seed_factories[0].source, "chakra-xyk");
        assert_eq!(config.discovery_factories[0].source, "discovered:clmm");

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
            if method == "eth_getCode" {
                // Manifest venue verification (SC-15): the seed factory has code.
                return Ok(json!("0x60"));
            }
            assert_eq!(method, "eth_call");
            let to = params[0]["to"].as_str().unwrap();
            let data = params[0]["data"].as_str().unwrap();
            if to.eq_ignore_ascii_case(XYK_FACTORY) {
                if data.contains(&usdc_word) && data.contains(&eurc_word) {
                    // getPair(USDC, EURC) → the seeded pool.
                    return Ok(json!(format!("0x{:0>64}", &pool[2..])));
                }
                return Ok(json!(word(0)));
            }
            if to.eq_ignore_ascii_case(POOL) {
                let t0_sel = dex_adapters::evm_fetch::token0_selector();
                let t1_sel = dex_adapters::evm_fetch::token1_selector();
                let res_sel = dex_adapters::evm_fetch::get_reserves_selector();
                if data.starts_with(&t0_sel) {
                    return Ok(json!(format!("0x{:0>64}", &USDC[2..])));
                }
                if data.starts_with(&t1_sel) {
                    return Ok(json!(format!("0x{:0>64}", &EURC[2..])));
                }
                if data.starts_with(&res_sel) {
                    return Ok(json!(reserves_response(10_000_000_000, 10_000_000_000)));
                }
            }
            Ok(json!(word(0)))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
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

    #[tokio::test]
    async fn discovery_finds_presto_hub_pair_from_seeded_hub() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| {
            if method == "eth_getCode" {
                return Ok(json!("0x60806040"));
            }
            if method == "eth_call" {
                let data = params[0]["data"].as_str().unwrap_or("");
                let path_usd_sel = dex_adapters::evm_fetch::path_usd_selector();
                let path_res_sel = dex_adapters::evm_fetch::path_reserves_selector();
                let token_res_sel = dex_adapters::evm_fetch::token_reserves_selector();
                if data.starts_with(&path_usd_sel) {
                    return Ok(json!(format!("0x{:0>64}", &USDC[2..])));
                }
                if data.starts_with(&path_res_sel) {
                    return Ok(json!(format!("0x{:0>64x}", 200_000_000_000u128)));
                }
                if data.starts_with(&token_res_sel) {
                    return Ok(json!(format!("0x{:0>64x}", 200_000_000_000u128)));
                }
            }
            Ok(json!(word(0)))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{PRESTO_HUB}:presto"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared.clone(), None);
        let found = runner.discover_once().await.unwrap();
        assert!(found >= 1, "seeded Presto hub must yield >= 1 pool, got {found}");
        let guard = shared.read().await;
        let presto_source = guard
            .sources
            .iter()
            .find(|s| s.source == "presto-hub")
            .expect("presto-hub source in topology");
        assert_eq!(presto_source.pairs.len(), 1);
        let pair = &presto_source.pairs[0];
        assert_eq!(pair.dex_type, "presto");
        assert_eq!(pair.fee_bps, 30);
        assert_eq!(pair.pool_address, PRESTO_HUB.to_ascii_lowercase());
        assert_eq!(pair.factory, PRESTO_HUB.to_ascii_lowercase());
    }

    // ─── T3.3 Five-check venue verification failure matrix ──────────

    #[tokio::test]
    async fn venue_five_checks_all_pass_published() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => Ok(json!("0x60")),
            "eth_call" => {
                let to = params[0]["to"].as_str().unwrap_or("");
                let data = params[0]["data"].as_str().unwrap_or("");
                if to.eq_ignore_ascii_case(XYK_FACTORY) {
                    return Ok(json!(format!("0x{:0>64}", &POOL[2..])));
                }
                if to.eq_ignore_ascii_case(POOL) {
                    if data.starts_with(&dex_adapters::evm_fetch::token0_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &USDC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::token1_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &EURC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::get_reserves_selector()) {
                        return Ok(json!(reserves_response(10_000_000_000, 10_000_000_000)));
                    }
                }
                Ok(json!(word(0)))
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 1);
        assert_eq!(runner.verified_factories.read().unwrap().len(), 1);
        assert_eq!(runner.verified_factories.read().unwrap()[0].source, "chakra-xyk");
    }

    #[tokio::test]
    async fn venue_five_checks_bytecode_fails_skipped() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => {
                let addr = params[0].as_str().unwrap_or("");
                if addr.eq_ignore_ascii_case(XYK_FACTORY) {
                    Ok(json!("0x60"))
                } else {
                    Ok(json!("0x")) // POOL has no bytecode
                }
            }
            "eth_call" => {
                let to = params[0]["to"].as_str().unwrap_or("");
                if to.eq_ignore_ascii_case(XYK_FACTORY) {
                    Ok(json!(format!("0x{:0>64}", &POOL[2..])))
                } else {
                    Ok(json!(word(0)))
                }
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 0, "pool with no bytecode must be skipped");
        assert_eq!(
            runner.verified_factories.read().unwrap().len(),
            0,
            "seed factory with no verified pools must not be in verified_factories"
        );
    }

    #[tokio::test]
    async fn venue_five_checks_canonical_endpoints_fail_skipped() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => Ok(json!("0x60")),
            "eth_call" => {
                let to = params[0]["to"].as_str().unwrap_or("");
                let data = params[0]["data"].as_str().unwrap_or("");
                if to.eq_ignore_ascii_case(XYK_FACTORY) {
                    return Ok(json!(format!("0x{:0>64}", &POOL[2..])));
                }
                if to.eq_ignore_ascii_case(POOL) {
                    if data.starts_with(&dex_adapters::evm_fetch::token0_selector()) {
                        // Mismatched token0 address
                        return Ok(json!(
                            "0x0000000000000000000000009999999999999999999999999999999999999999"
                        ));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::token1_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &EURC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::get_reserves_selector()) {
                        return Ok(json!(reserves_response(10_000_000_000, 10_000_000_000)));
                    }
                }
                Ok(json!(word(0)))
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 0, "pool with mismatched token endpoints must be skipped");
        assert_eq!(runner.verified_factories.read().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn venue_five_checks_factory_membership_fails_skipped() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => Ok(json!("0x60")),
            "eth_call" => {
                let to = params[0]["to"].as_str().unwrap_or("");
                if to.eq_ignore_ascii_case(XYK_FACTORY) {
                    // getPair returns address(0)
                    return Ok(json!(word(0)));
                }
                Ok(json!(word(0)))
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 0, "missing factory membership must yield 0 pools");
        assert_eq!(runner.verified_factories.read().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn venue_five_checks_nonzero_reserves_fail_skipped() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => Ok(json!("0x60")),
            "eth_call" => {
                let to = params[0]["to"].as_str().unwrap_or("");
                let data = params[0]["data"].as_str().unwrap_or("");
                if to.eq_ignore_ascii_case(XYK_FACTORY) {
                    return Ok(json!(format!("0x{:0>64}", &POOL[2..])));
                }
                if to.eq_ignore_ascii_case(POOL) {
                    if data.starts_with(&dex_adapters::evm_fetch::token0_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &USDC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::token1_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &EURC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::get_reserves_selector()) {
                        // Zero reserves
                        return Ok(json!(reserves_response(0, 0)));
                    }
                }
                Ok(json!(word(0)))
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 0, "pool with zero reserves must be skipped");
        assert_eq!(runner.verified_factories.read().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn venue_five_checks_probe_quote_fails_skipped() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => Ok(json!("0x60")),
            "eth_call" => {
                let to = params[0]["to"].as_str().unwrap_or("");
                let data = params[0]["data"].as_str().unwrap_or("");
                if to.eq_ignore_ascii_case(XYK_FACTORY) {
                    return Ok(json!(format!("0x{:0>64}", &POOL[2..])));
                }
                if to.eq_ignore_ascii_case(POOL) {
                    if data.starts_with(&dex_adapters::evm_fetch::token0_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &USDC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::token1_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &EURC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::get_reserves_selector()) {
                        // Both reserves are non-zero (passes check 4), but extreme ratio makes integer quote output 0 (fails check 5)
                        return Ok(json!(reserves_response(100_000_000_000_000_000, 1)));
                    }
                }
                Ok(json!(word(0)))
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 0, "pool with failing probe quote must be skipped");
        assert_eq!(runner.verified_factories.read().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn venue_five_checks_xylo_all_pass_published() {
        const XYLO_FACTORY: &str = "0x60EDeFB094B84BBC6430cc130B358A43Ba1979e2";
        const XYLO_POOL: &str = "0x3DF3966F5138143dce7a9cFDdC2c0310ce083BB1";
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => Ok(json!("0x60")),
            "eth_call" => {
                let to = params[0]["to"].as_str().unwrap_or("");
                let data = params[0]["data"].as_str().unwrap_or("");
                if to.eq_ignore_ascii_case(XYLO_FACTORY) {
                    return Ok(json!(format!("0x{:0>64}", &XYLO_POOL[2..])));
                }
                if to.eq_ignore_ascii_case(XYLO_POOL) {
                    if data.starts_with(&dex_adapters::evm_fetch::token0_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &USDC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::token1_selector()) {
                        return Ok(json!(format!("0x{:0>64}", &EURC[2..])));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::get_reserves_selector()) {
                        return Ok(json!(reserves_response(9_323_185_000_000, 9_323_185_000_000)));
                    }
                    if data.starts_with(&dex_adapters::evm_fetch::get_amplification_selector()) {
                        return Ok(json!(format!("0x{:0>64x}", 20000u128)));
                    }
                }
                Ok(json!(word(0)))
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYLO_FACTORY}:xylo"), true).unwrap()],
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
        assert_eq!(guard.sources[0].source, "xylo-stable");
        assert_eq!(guard.sources[0].pairs.len(), 1);
        let pair = &guard.sources[0].pairs[0];
        assert_eq!(pair.dex_type, "xylo");
        assert_eq!(pair.pool_address, XYLO_POOL.to_ascii_lowercase());
        assert_eq!(runner.verified_factories.read().unwrap().len(), 1);
        assert_eq!(runner.verified_factories.read().unwrap()[0].source, "xylo-stable");
    }

    #[tokio::test]
    async fn presto_five_checks_path_usd_endpoint_fails_skipped() {
        let (url, _server) = spawn_fixture_rpc(move |method, params| match method {
            "eth_getCode" => Ok(json!("0x60806040")),
            "eth_call" => {
                let data = params[0]["data"].as_str().unwrap_or("");
                if data.starts_with(&dex_adapters::evm_fetch::path_usd_selector()) {
                    // Mismatched pathUSD (returns non-USDC)
                    return Ok(json!(format!("0x{:0>64}", &EURC[2..])));
                }
                if data.starts_with(&dex_adapters::evm_fetch::path_reserves_selector()) {
                    return Ok(json!(format!("0x{:0>64x}", 200_000_000_000u128)));
                }
                if data.starts_with(&dex_adapters::evm_fetch::token_reserves_selector()) {
                    return Ok(json!(format!("0x{:0>64x}", 200_000_000_000u128)));
                }
                Ok(json!(word(0)))
            }
            _ => Ok(json!(word(0))),
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{PRESTO_HUB}:presto"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 0, "presto hub with wrong pathUSD must be skipped");
        assert_eq!(runner.verified_factories.read().unwrap().len(), 0);
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
                            json!({"jsonrpc": "2.0", "id": id, "result": "0xsub1"}).to_string(),
                        ))
                        .await
                        .unwrap();
                        ws.send(Message::Text(
                            json!({
                                "jsonrpc": "2.0",
                                "method": "eth_subscription",
                                "params": {"subscription": "0xsub1", "result": swap}
                            })
                            .to_string(),
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

    /// Sanity: discovery probes the canonical catalog pairs (USDC/EURC and
    /// cirBTC pairs) and finds nothing when the factory has no pools.
    #[tokio::test]
    async fn discovery_with_no_pools_probes_catalog_and_finds_zero() {
        let (url, _server) = spawn_fixture_rpc(|method, _params| {
            match method {
                "eth_getCode" => Ok(json!("0x60")), // verified venue
                _ => Ok(json!(word(0))),            // no pool for any pair
            }
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        // The canonical catalog (USDC/EURC/cirBTC) is always probed.
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared, None);
        assert_eq!(runner.discover_once().await.unwrap(), 0);
    }

    /// SC-15 (2026-08-29): a manifest venue whose factory has no bytecode is
    /// marked unavailable — it is never probed, never published, and yields
    /// zero pools (the API then returns NO_ROUTE; never auto-reseeded).
    #[tokio::test]
    async fn discovery_marks_bytecode_less_seed_factory_unavailable() {
        let (url, _server) = spawn_fixture_rpc(|method, _params| {
            match method {
                // No code at the seed factory → verification fails.
                "eth_getCode" => Ok(json!("0x")),
                other => Err(json!(format!("unexpected method {other}"))),
            }
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let config = EvmConfig {
            seed_factories: vec![FactoryConfig::parse(&format!("{XYK_FACTORY}:xyk"), true).unwrap()],
            ws_enabled: false,
            ..Default::default()
        };
        let shared = Arc::new(RwLock::new(WorkerShared {
            sources: Vec::new(),
            clmm_pools: Vec::new(),
        }));
        let mut runner = EvmRunner::new(config, client, shared.clone(), None);
        let found = runner.discover_once().await.unwrap();
        assert_eq!(found, 0, "unverified venue must not produce pools");
        assert_eq!(
            runner.verified_factories.read().unwrap().len(),
            0,
            "unverified venue must not be in the published factory set"
        );
    }
}
