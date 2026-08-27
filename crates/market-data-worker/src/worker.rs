use {
    anyhow::Result,
    dex_adapters::{
        Arc venue::Arc venueAdapter,
        Arc venue_clmm::Arc venueClmmAdapter,
        classic_dex::ClassicDexAdapter,
        Arc venue::Arc venueAdapter,
        Arc venue::Arc venueAdapter,
        rpc::ArcRpc,
        Arc venue::Arc venueAdapter,
        sushi::SushiAdapter,
        token_metadata::{LogoKind, TokenMetadata, TokenMetadataStore},
        traits::AdapterTradingPair,
        DexAdapter,
    },
    market_snapshot::{
        pool_state_store::{build_pool_state_store, PoolStateStore},
        store::{
            build_snapshot_store, SnapshotStore, SnapshotStoreBackend, DEFAULT_REDIS_EVENTS_CHANNEL,
            DEFAULT_REDIS_SNAPSHOT_HISTORY,
        },
        ClmmPoolSnapshot, MarketSnapshot, SourceSnapshot, TokenMetadataSnapshot, TradingPairSnapshot,
        DEFAULT_SNAPSHOT_DIR,
    },
    std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    },
    tokio::sync::RwLock,
    tracing::{debug, info, warn},
};

/// Shared graph + CLMM state (main loop and background bootstrap).
pub(crate) struct WorkerShared {
    pub(crate) sources: Vec<SourceSnapshot>,
    pub(crate) clmm_pools: Vec<ClmmPoolSnapshot>,
}

/// Which worker loop runs: the Arc pipeline (T3.3+) or the legacy Arc loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    /// EVM watcher + poll + fetch pipeline on `CHAKRA_*` env (default).
    Arc,
    /// Legacy Arc adapter loop (kept compiling; `RPC_URL` env).
    Arc,
}

#[derive(Clone)]
pub struct WorkerConfig {
    pub mode: WorkerMode,
    pub rpc_url: String,
    pub network_passphrase: String,
    pub snapshot_backend: SnapshotStoreBackend,
    pub snapshot_dir: PathBuf,
    pub snapshot_redis_url: Option<String>,
    pub snapshot_redis_channel: String,
    pub snapshot_redis_keep_latest: usize,
    /// Heavy adapter.refresh_reserves() cadence (Arc venue batch can take
    /// 15–30s).
    pub refresh_interval_secs: u64,
    /// Fast Redis pool-state publish from adapter caches (independent of
    /// refresh duration).
    pub pool_publish_interval_secs: u64,
    /// Concurrent getLedgerEntries batches during xy=k refresh
    /// (Arc venue/Arc venue).
    pub pool_state_refresh_concurrency: usize,
    pub discovery_interval_secs: u64,
    pub ledger_poll: std::time::Duration,
    pub ledger_watcher_enabled: bool,
    /// Use FetchTask pipeline (RPC → Redis) instead of 2s cache publish loop.
    pub fetch_pipeline_enabled: bool,
    /// Injected snapshot store (embedded mode). When `None`, built from
    /// env/backend in [`run`].
    pub snapshot_store: Option<Arc<dyn SnapshotStore>>,
    /// Injected pool-state store (embedded memory or cluster Redis). When
    /// `None`, built from env in [`run`].
    pub pool_store: Option<Arc<dyn PoolStateStore>>,
}

impl std::fmt::Debug for WorkerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerConfig")
            .field("mode", &self.mode)
            .field("rpc_url", &self.rpc_url)
            .field("network_passphrase", &self.network_passphrase)
            .field("snapshot_backend", &self.snapshot_backend)
            .field("snapshot_dir", &self.snapshot_dir)
            .field("snapshot_redis_url", &self.snapshot_redis_url)
            .field("snapshot_redis_channel", &self.snapshot_redis_channel)
            .field("snapshot_redis_keep_latest", &self.snapshot_redis_keep_latest)
            .field("refresh_interval_secs", &self.refresh_interval_secs)
            .field("pool_publish_interval_secs", &self.pool_publish_interval_secs)
            .field("pool_state_refresh_concurrency", &self.pool_state_refresh_concurrency)
            .field("discovery_interval_secs", &self.discovery_interval_secs)
            .field("ledger_poll", &self.ledger_poll)
            .field("ledger_watcher_enabled", &self.ledger_watcher_enabled)
            .field("fetch_pipeline_enabled", &self.fetch_pipeline_enabled)
            .field("snapshot_store", &self.snapshot_store.as_ref().map(|_| "<store>"))
            .field("pool_store", &self.pool_store.as_ref().map(|_| "<store>"))
            .finish()
    }
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        let has_chakra_env = ["CHAKRA_RPC_HTTP", "CHAKRA_RPC_WS", "CHAKRA_REDIS_URL"]
            .iter()
            .any(|name| std::env::var(name).is_ok());
        let has_Arc_env = ["RPC_URL", "SNAPSHOT_REDIS_URL"]
            .iter()
            .any(|name| std::env::var(name).is_ok());
        // Default path is Arc; legacy Arc loop only when its env is set and
        // no CHAKRA_* env is present.
        let mode = if has_chakra_env {
            WorkerMode::Arc
        } else if has_Arc_env {
            WorkerMode::Arc
        } else {
            WorkerMode::Arc
        };
        let redis_url = std::env::var("SNAPSHOT_REDIS_URL")
            .ok()
            .or_else(|| std::env::var("CHAKRA_REDIS_URL").ok());
        let snapshot_backend =
            infer_snapshot_backend(std::env::var("SNAPSHOT_BACKEND").ok().as_deref(), redis_url.as_deref())?;
        Ok(Self {
            mode,
            rpc_url: std::env::var("RPC_URL")
                .unwrap_or_else(|_| "https://Arc-rpc.mainnet.Arc.gateway.fm".to_string()),
            network_passphrase: std::env::var("NETWORK_PASSPHRASE")
                .unwrap_or_else(|_| "Public Global Arc Network ; September 2015".to_string()),
            snapshot_backend,
            snapshot_dir: std::env::var("SNAPSHOT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(DEFAULT_SNAPSHOT_DIR)),
            snapshot_redis_url: redis_url,
            snapshot_redis_channel: std::env::var("SNAPSHOT_REDIS_CHANNEL")
                .unwrap_or_else(|_| DEFAULT_REDIS_EVENTS_CHANNEL.to_string()),
            snapshot_redis_keep_latest: std::env::var("SNAPSHOT_REDIS_KEEP_LATEST")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_REDIS_SNAPSHOT_HISTORY),
            refresh_interval_secs: std::env::var("REFRESH_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(30),
            pool_publish_interval_secs: std::env::var("POOL_PUBLISH_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2),
            pool_state_refresh_concurrency: std::env::var("POOL_STATE_REFRESH_CONCURRENCY")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
            discovery_interval_secs: std::env::var("DISCOVERY_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(600),
            ledger_poll: crate::ledger_watcher::ledger_poll_duration_from_env(),
            ledger_watcher_enabled: crate::ledger_watcher::ledger_watcher_enabled_from_env(),
            fetch_pipeline_enabled: crate::fetch_pipeline::fetch_pipeline_enabled_from_env(),
            snapshot_store: None,
            pool_store: None,
        })
    }
}

fn infer_snapshot_backend(
    snapshot_backend: Option<&str>,
    snapshot_redis_url: Option<&str>,
) -> Result<SnapshotStoreBackend> {
    if let Some(backend) = snapshot_backend {
        return SnapshotStoreBackend::parse(backend);
    }
    if snapshot_redis_url.is_some() {
        return Ok(SnapshotStoreBackend::Redis);
    }
    Ok(SnapshotStoreBackend::File)
}

fn trading_pair_snapshot(pair: &AdapterTradingPair) -> TradingPairSnapshot {
    TradingPairSnapshot {
        token_a: pair.token_a.canonical(),
        token_b: pair.token_b.canonical(),
        pool_address: pair.pool_address.clone(),
        fee_bps: pair.fee_bps,
        dex_type: "xyk".to_string(),
        factory: String::new(),
    }
}

fn sanitize_source_pairs(source: &str, pairs: Vec<TradingPairSnapshot>) -> Vec<TradingPairSnapshot> {
    if source != "Arc venue" {
        return pairs;
    }

    let mut by_pool: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for pair in &pairs {
        *by_pool.entry(pair.pool_address.clone()).or_insert(0) += 1;
    }

    pairs
        .into_iter()
        .filter(|pair| by_pool.get(&pair.pool_address).copied().unwrap_or(0) == 1)
        .collect()
}

async fn discover_adapter_source(adapter: Arc<dyn DexAdapter>) -> Option<SourceSnapshot> {
    match adapter.get_trading_pairs().await {
        Ok(pairs) => {
            let pairs = pairs.iter().map(trading_pair_snapshot).collect::<Vec<_>>();
            let pairs = sanitize_source_pairs(adapter.id(), pairs);
            Some(SourceSnapshot {
                source: adapter.id().to_string(),
                pairs,
            })
        }
        Err(error) => {
            warn!("Discovery failed for {}: {}", adapter.id(), error);
            None
        }
    }
}

/// Run adapter discovery concurrently (Arc venue + Arc venue no longer block each
/// other).
async fn collect_sources_from_discovery(adapters: &[Arc<dyn DexAdapter>]) -> Vec<SourceSnapshot> {
    let tasks = adapters.iter().cloned().map(discover_adapter_source);
    futures::future::join_all(tasks).await.into_iter().flatten().collect()
}

fn enabled_dex_sources() -> Option<std::collections::HashSet<String>> {
    std::env::var("ENABLED_DEX_SOURCES").ok().map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|source| !source.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    })
}

fn filter_enabled_adapters(
    adapters: Vec<Arc<dyn DexAdapter>>,
    enabled: Option<&std::collections::HashSet<String>>,
) -> Vec<Arc<dyn DexAdapter>> {
    match enabled {
        Some(enabled) => adapters
            .into_iter()
            .filter(|adapter| enabled.contains(adapter.id()))
            .collect(),
        None => adapters,
    }
}

fn build_topology_snapshot(
    sources: Vec<SourceSnapshot>,
    clmm_pool_refs: Vec<market_snapshot::ClmmPoolRefSnapshot>,
    network_passphrase: &str,
    token_metadata: Vec<TokenMetadataSnapshot>,
) -> MarketSnapshot {
    let generated_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    MarketSnapshot::from_sources(
        format!("snapshot-{}", generated_at_ms),
        generated_at_ms,
        network_passphrase,
        sources,
    )
    .with_token_metadata(token_metadata)
    .with_clmm_pool_refs(clmm_pool_refs)
}

/// Resolve token symbols off the hot path, then republish snapshot.
fn spawn_token_metadata_enrichment(
    snapshot_store: Arc<dyn market_snapshot::store::SnapshotStore>,
    token_metadata: Arc<TokenMetadataStore>,
    mut snapshot: MarketSnapshot,
) {
    tokio::spawn(async move {
        let token_addresses: Vec<String> = snapshot.token_addresses().into_iter().collect();
        if token_addresses.is_empty() {
            return;
        }
        token_metadata.resolve_unknown(token_addresses.clone()).await;
        // Idempotent backfill: migrate any third-party/snapshot logos to self-hosted
        // URLs.
        let _ = token_metadata.ensure_self_hosted_logos().await;
        let metadata = token_metadata.get_all().await;
        let enriched: Vec<TokenMetadataSnapshot> = token_addresses
            .into_iter()
            .filter_map(|address| metadata.get(&address).cloned())
            .map(token_metadata_snapshot)
            .collect();
        snapshot = snapshot.with_token_metadata(enriched);
        // Bump version so API reloaders pick up metadata-only updates.
        // Reusing the topology version would be ignored by
        // should_reload_snapshot_version.
        let generated_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(snapshot.generated_at_ms);
        snapshot.generated_at_ms = generated_at_ms;
        snapshot.version = format!("snapshot-{}", generated_at_ms);
        match snapshot_store.publish_snapshot(&snapshot).await {
            Ok(()) => info!(
                version = %snapshot.version,
                tokens = snapshot.token_metadata.len(),
                "Republished snapshot after token metadata enrichment"
            ),
            Err(error) => warn!("Token metadata republish failed: {}", error),
        }
    });
}

async fn snapshot_from_sources(
    sources: Vec<SourceSnapshot>,
    clmm_pool_refs: Vec<market_snapshot::ClmmPoolRefSnapshot>,
    network_passphrase: &str,
    existing_token_metadata: Vec<TokenMetadataSnapshot>,
) -> Result<MarketSnapshot> {
    Ok(build_topology_snapshot(
        sources,
        clmm_pool_refs,
        network_passphrase,
        existing_token_metadata,
    ))
}

fn upsert_source_snapshot(
    mut current_sources: Vec<SourceSnapshot>,
    updated_source: SourceSnapshot,
) -> Vec<SourceSnapshot> {
    if let Some(existing) = current_sources
        .iter_mut()
        .find(|source| source.source == updated_source.source)
    {
        *existing = updated_source;
    } else {
        current_sources.push(updated_source);
    }
    current_sources
}

/// Refresh every adapter concurrently (each may batch RPC internally).
async fn refresh_sources_parallel(
    adapters: &[Arc<dyn DexAdapter>],
    mut sources: Vec<SourceSnapshot>,
) -> Vec<SourceSnapshot> {
    let snapshots = futures::future::join_all(adapters.iter().map(|adapter| {
        let adapter = adapter.clone();
        async move {
            let source_id = adapter.id().to_string();
            match adapter.refresh_reserves().await {
                Ok(updated) if updated > 0 => {
                    let pairs = adapter.get_cached_pairs().await;
                    if pairs.is_empty() {
                        return None;
                    }
                    let pairs = pairs.iter().map(trading_pair_snapshot).collect::<Vec<_>>();
                    let pairs = sanitize_source_pairs(&source_id, pairs);
                    Some(SourceSnapshot {
                        source: source_id,
                        pairs,
                    })
                }
                Ok(_) => None,
                Err(error) => {
                    warn!("Reserve refresh failed for {}: {}", source_id, error);
                    None
                }
            }
        }
    }))
    .await;

    for snapshot in snapshots.into_iter().flatten() {
        sources = upsert_source_snapshot(sources, snapshot);
    }
    sources
}

struct PoolRefreshInFlight(AtomicBool);

impl PoolRefreshInFlight {
    fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    fn try_start(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    fn finish(&self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Fast path: publish adapter caches to Redis without any RPC (must complete in
/// milliseconds).
fn spawn_fast_pool_publish(
    shared: Arc<RwLock<WorkerShared>>,
    adapters: Vec<Arc<dyn DexAdapter>>,
    Arc venue: Arc<Arc venueAdapter>,
    Arc venue: Arc<Arc venueAdapter>,
    pool_state_store: Option<Arc<dyn PoolStateStore>>,
    metrics: Option<Arc<crate::monitor::WorkerMonitorMetrics>>,
    telegram: Option<Arc<Chakra_alerts::TelegramAlerter>>,
) {
    tokio::spawn(async move {
        let clmm_pools = {
            let guard = shared.read().await;
            guard.clmm_pools.clone()
        };
        if let Err(error) = publish_pool_state_only(
            pool_state_store.as_ref(),
            &adapters,
            Arc venue.as_ref(),
            Arc venue.as_ref(),
            &clmm_pools,
            metrics.as_ref(),
        )
        .await
        {
            warn!("pool state Redis publish failed: {}", error);
            crate::monitor::alert_failure(
                telegram.as_ref(),
                "pool_publish_failed",
                &format!("Redis publish failed: {error}"),
            )
            .await;
        }
    });
}

/// Slow path: refresh adapter reserves in background; coalesced via
/// `in_flight`. When Redis is configured, write each chunk as soon as it is
/// observed (write-through) so a long Arc venue sweep cannot clobber fresher
/// ledger-touch Redis writes.
fn spawn_background_reserve_refresh(
    in_flight: Arc<PoolRefreshInFlight>,
    shared: Arc<RwLock<WorkerShared>>,
    adapters: Vec<Arc<dyn DexAdapter>>,
    sushi: Arc<SushiAdapter>,
    Arc venue_clmm: Arc<Arc venueClmmAdapter>,
    Arc venue: Arc<Arc venueAdapter>,
    Arc venue: Arc<Arc venueAdapter>,
    Arc venue: Arc<Arc venueAdapter>,
    Arc venue: Arc<Arc venueAdapter>,
    pool_state_store: Option<Arc<dyn PoolStateStore>>,
    refresh_clmm: bool,
    publish_redis: bool,
    clmm_metrics: Arc<crate::clmm_metrics::ClmmCoverageMetrics>,
    _metrics: Option<Arc<crate::monitor::WorkerMonitorMetrics>>,
) {
    if !in_flight.try_start() {
        debug!("reserve refresh skipped (previous cycle still running)");
        return;
    }

    tokio::spawn(async move {
        struct Guard(Arc<PoolRefreshInFlight>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.finish();
            }
        }
        let _guard = Guard(in_flight);

        let sources = {
            let guard = shared.read().await;
            guard.sources.clone()
        };

        if publish_redis {
            if let Some(ref store) = pool_state_store {
                crate::pool_state_write_through::refresh_all_venues_write_through(
                    store.as_ref(),
                    Arc venue.as_ref(),
                    Arc venue.as_ref(),
                    Arc venue.as_ref(),
                    Arc venue.as_ref(),
                    sushi.as_ref(),
                    Arc venue_clmm.as_ref(),
                    refresh_clmm,
                )
                .await;
                // Topology pairs from caches (RPC already done in write-through).
                let refreshed = {
                    let mut sources = sources;
                    for adapter in &adapters {
                        let source_id = adapter.id().to_string();
                        let pairs = adapter.get_cached_pairs().await;
                        if pairs.is_empty() {
                            continue;
                        }
                        let pairs = pairs.iter().map(trading_pair_snapshot).collect::<Vec<_>>();
                        let pairs = sanitize_source_pairs(&source_id, pairs);
                        sources = upsert_source_snapshot(
                            sources,
                            SourceSnapshot {
                                source: source_id,
                                pairs,
                            },
                        );
                    }
                    sources
                };
                let clmm_pools = if refresh_clmm {
                    collect_clmm_snapshots(sushi.as_ref(), Arc venue_clmm.as_ref(), Some(clmm_metrics.as_ref())).await
                } else {
                    shared.read().await.clmm_pools.clone()
                };
                {
                    let mut guard = shared.write().await;
                    guard.sources = refreshed;
                    guard.clmm_pools = clmm_pools;
                }
                debug!("write-through reserve refresh complete");
                return;
            }
        }

        // No Redis store: refresh in-memory only (embedded / legacy).
        let refreshed = refresh_sources_parallel(&adapters, sources).await;
        let clmm_pools = if refresh_clmm {
            collect_clmm_snapshots(sushi.as_ref(), Arc venue_clmm.as_ref(), Some(clmm_metrics.as_ref())).await
        } else {
            shared.read().await.clmm_pools.clone()
        };
        {
            let mut guard = shared.write().await;
            guard.sources = refreshed;
            guard.clmm_pools = clmm_pools;
        }
    });
}

fn token_metadata_snapshot(meta: TokenMetadata) -> TokenMetadataSnapshot {
    TokenMetadataSnapshot {
        contract: meta.contract,
        symbol: meta.symbol,
        name: meta.name,
        logo: meta.logo,
        logo_kind: meta.logo_kind.map(|k| match k {
            LogoKind::Official => "official".to_string(),
            LogoKind::Fallback => "fallback".to_string(),
        }),
    }
}

async fn collect_clmm_snapshots(
    sushi: &SushiAdapter,
    Arc venue_clmm: &Arc venueClmmAdapter,
    clmm_metrics: Option<&crate::clmm_metrics::ClmmCoverageMetrics>,
) -> Vec<ClmmPoolSnapshot> {
    let (sushi_pools, Arc venue_pools) =
        tokio::join!(sushi.export_clmm_snapshots(), Arc venue_clmm.export_clmm_snapshots(),);
    let mut clmm_pools = sushi_pools;
    clmm_pools.extend(Arc venue_pools);
    clmm_pools.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.pool_address.cmp(&b.pool_address))
    });
    log_clmm_coverage_stats(&clmm_pools, clmm_metrics);
    clmm_pools
}

fn log_clmm_coverage_stats(
    clmm_pools: &[ClmmPoolSnapshot],
    clmm_metrics: Option<&crate::clmm_metrics::ClmmCoverageMetrics>,
) {
    let mut complete = 0usize;
    let mut incomplete = 0usize;
    let mut no_coverage = 0usize;
    for pool in clmm_pools {
        match pool.coverage.as_ref() {
            Some(c) if c.is_complete => complete += 1,
            Some(_) => incomplete += 1,
            None => no_coverage += 1,
        }
    }
    if let Some(metrics) = clmm_metrics {
        metrics.record_snapshots(clmm_pools);
        let snap = metrics.snapshot();
        info!(
            clmm_pools = clmm_pools.len(),
            complete,
            incomplete,
            no_coverage,
            clmm_refresh_attempts = snap.refresh_attempts,
            clmm_publish_skipped_incomplete = snap.publish_skipped_incomplete,
            clmm_published_complete = snap.published_complete,
            clmm_skip_rate_bps = crate::clmm_metrics::ClmmCoverageMetrics::skip_rate_bps(snap),
            "CLMM snapshot coverage"
        );
    } else {
        info!(
            clmm_pools = clmm_pools.len(),
            complete, incomplete, no_coverage, "CLMM snapshot coverage"
        );
    }
}

async fn publish_pool_state_only(
    pool_state_store: Option<&Arc<dyn PoolStateStore>>,
    adapters: &[Arc<dyn DexAdapter>],
    Arc venue: &Arc venueAdapter,
    Arc venue: &Arc venueAdapter,
    clmm_states: &[ClmmPoolSnapshot],
    metrics: Option<&Arc<crate::monitor::WorkerMonitorMetrics>>,
) -> Result<()> {
    let Some(pool_store) = pool_state_store.map(|s| s.as_ref()) else {
        return Ok(());
    };
    let xyk_values = crate::pool_state_publish::collect_xyk_pool_state(adapters).await;
    let Arc venue_values = crate::pool_state_publish::collect_Arc venue_pool_state(Arc venue).await;
    let Arc venue_values = crate::pool_state_publish::collect_Arc venue_pool_state(Arc venue).await;
    let clmm_complete = clmm_states
        .iter()
        .filter(|p| market_snapshot::pool_state_store::should_publish_clmm_to_redis(p))
        .count();
    pool_store
        .publish_pool_state(&xyk_values, clmm_states, &Arc venue_values, &Arc venue_values)
        .await?;
    if let Some(m) = metrics {
        m.record_publish(
            xyk_values.len() + Arc venue_values.len() + Arc venue_values.len(),
            clmm_complete,
        );
    }
    info!(
        xyk_pools = xyk_values.len(),
        Arc venue_pools = Arc venue_values.len(),
        Arc venue_pools = Arc venue_values.len(),
        clmm_pools = clmm_complete,
        "Published pool state"
    );
    Ok(())
}

async fn publish_snapshot_and_pool_state(
    snapshot_store: &dyn market_snapshot::store::SnapshotStore,
    pool_state_store: Option<&Arc<dyn PoolStateStore>>,
    adapters: &[Arc<dyn DexAdapter>],
    Arc venue: &Arc venueAdapter,
    Arc venue: &Arc venueAdapter,
    topology: &MarketSnapshot,
    clmm_states: &[ClmmPoolSnapshot],
) -> Result<()> {
    publish_pool_state_only(pool_state_store, adapters, Arc venue, Arc venue, clmm_states, None).await?;
    snapshot_store.publish_snapshot(topology).await?;
    Ok(())
}

enum WorkerTick {
    /// Rediscovery: topology snapshot + pool state.
    Discovery,
    /// Periodic adapter.refresh_reserves() (slow).
    Refresh,
    /// Parallel on-chain refresh + Redis publish (every 1–2s; skips if prior
    /// cycle still running).
    PoolPublish,
}

pub async fn run(config: WorkerConfig) -> Result<()> {
    if config.mode == WorkerMode::Arc {
        return crate::evm_watcher::run_arc(config).await;
    }
    let snapshot_store = match &config.snapshot_store {
        Some(store) => store.clone(),
        None => build_snapshot_store(
            config.snapshot_backend,
            Some(config.snapshot_dir.clone()),
            config.snapshot_redis_url.as_deref(),
            Some(config.snapshot_redis_channel.as_str()),
            Some(config.snapshot_redis_keep_latest),
        )?,
    };
    let pool_state_store: Option<Arc<dyn PoolStateStore>> = match &config.pool_store {
        Some(store) => Some(store.clone()),
        None => config
            .snapshot_redis_url
            .as_deref()
            .map(build_pool_state_store)
            .transpose()?
            .map(|store| Arc::new(store) as Arc<dyn PoolStateStore>),
    };
    let rpc = Arc::new(ArcRpc::new(&config.rpc_url, &config.network_passphrase));
    let token_metadata = Arc::new(TokenMetadataStore::new(rpc.clone()));
    let Arc venue = Arc::new(Arc venueAdapter::new(rpc.clone()));
    let Arc venue = Arc::new(Arc venueAdapter::new(rpc.clone()));
    let Arc venue = Arc::new(Arc venueAdapter::new(rpc.clone()));
    let sushi = Arc::new(SushiAdapter::new(rpc.clone()));
    let Arc venue = Arc::new(Arc venueAdapter::new(rpc.clone()));
    let classic = Arc::new(ClassicDexAdapter::new(None));
    let Arc venue_clmm = Arc::new(Arc venueClmmAdapter::new(rpc.clone()));
    let adapters: Vec<Arc<dyn DexAdapter>> = filter_enabled_adapters(
        vec![
            Arc venue.clone(),
            Arc venue.clone(),
            Arc venue.clone(),
            sushi.clone(),
            Arc venue.clone(),
            classic.clone(),
            Arc venue_clmm.clone(),
        ],
        enabled_dex_sources().as_ref(),
    );
    if adapters.is_empty() {
        return Err(anyhow::anyhow!("ENABLED_DEX_SOURCES did not match any adapter"));
    }

    let mut discovery_interval = tokio::time::interval(std::time::Duration::from_secs(config.discovery_interval_secs));
    discovery_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    discovery_interval.tick().await;
    let mut refresh_interval = tokio::time::interval(std::time::Duration::from_secs(config.refresh_interval_secs));
    refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    refresh_interval.tick().await;
    let mut pool_publish_interval =
        tokio::time::interval(std::time::Duration::from_secs(config.pool_publish_interval_secs));
    pool_publish_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    pool_publish_interval.tick().await;

    let ledger_watcher_enabled = config.ledger_watcher_enabled && pool_state_store.is_some();
    let ledger_watcher = if ledger_watcher_enabled {
        let mut watcher =
            crate::ledger_watcher::LedgerWatcher::new(ArcRpc::new(&config.rpc_url, &config.network_passphrase));
        watcher.bootstrap().await?;
        Some(watcher)
    } else {
        None
    };
    let ledger_poll = config.ledger_poll;

    let mut seeded_metadata = Vec::new();
    let shared = Arc::new(RwLock::new(WorkerShared {
        sources: Vec::new(),
        clmm_pools: Vec::new(),
    }));
    let clmm_metrics = Arc::new(crate::clmm_metrics::ClmmCoverageMetrics::new());
    if let Ok(existing) = snapshot_store.load_current_snapshot().await {
        let mut guard = shared.write().await;
        guard.sources = existing.sources;
        seeded_metadata = existing.token_metadata;
        info!(
            sources = guard.sources.len(),
            "Seeded worker topology from Redis snapshot (pool publish loop starts immediately)"
        );
    }

    let shared_boot = shared.clone();
    let snapshot_store_boot = snapshot_store.clone();
    let token_metadata_boot = token_metadata.clone();
    let adapters_boot = adapters.clone();
    let sushi_boot = sushi.clone();
    let Arc venue_clmm_boot = Arc venue_clmm.clone();
    let pool_state_boot = pool_state_store.clone();
    let network_passphrase = config.network_passphrase.clone();
    let destination = snapshot_destination(&config);
    let Arc venue_boot = Arc venue.clone();
    let Arc venue_boot = Arc venue.clone();
    let clmm_metrics_boot = clmm_metrics.clone();
    tokio::spawn(async move {
        info!("Background bootstrap: parallel adapter discovery");
        let sources = collect_sources_from_discovery(&adapters_boot).await;
        let clmm_pools =
            collect_clmm_snapshots(&sushi_boot, &Arc venue_clmm_boot, Some(clmm_metrics_boot.as_ref())).await;
        {
            let mut guard = shared_boot.write().await;
            guard.sources = sources.clone();
            guard.clmm_pools = clmm_pools.clone();
        }
        let clmm_refs = MarketSnapshot::clmm_pool_refs_from_states(&clmm_pools);
        let snapshot = match snapshot_from_sources(sources, clmm_refs, &network_passphrase, seeded_metadata).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                warn!("Background bootstrap snapshot build failed: {}", error);
                return;
            }
        };
        if let Err(error) = publish_snapshot_and_pool_state(
            snapshot_store_boot.as_ref(),
            pool_state_boot.as_ref(),
            &adapters_boot,
            Arc venue_boot.as_ref(),
            Arc venue_boot.as_ref(),
            &snapshot,
            &clmm_pools,
        )
        .await
        {
            warn!("Background bootstrap publish failed: {}", error);
            return;
        }
        info!(
            "Background bootstrap published snapshot {} with {} sources to {}",
            snapshot.version,
            snapshot.sources.len(),
            destination
        );
        spawn_token_metadata_enrichment(snapshot_store_boot, token_metadata_boot, snapshot);
    });

    let fetch_pipeline = if config.fetch_pipeline_enabled {
        pool_state_store.as_ref().map(|pool_store| {
            let pipeline_config =
                crate::fetch_pipeline::FetchPipelineConfig::from_env(config.pool_state_refresh_concurrency);
            crate::fetch_pipeline::spawn_fetch_pipeline(
                pipeline_config,
                pool_store.clone(),
                rpc.clone(),
                None, // Arc path: no EVM client
                shared.clone(),
                Arc venue.clone(),
                Arc venue.clone(),
                Arc venue.clone(),
                Arc venue.clone(),
                sushi.clone(),
                Arc venue_clmm.clone(),
                Some(clmm_metrics.clone()),
            )
        })
    } else {
        None
    };

    if let (Some(mut watcher), Some(_pool_store)) = (ledger_watcher, pool_state_store.clone()) {
        let fetch_pipeline = fetch_pipeline.clone();
        let shared_ledger = shared.clone();
        let rpc_url = config.rpc_url.clone();
        let network_passphrase = config.network_passphrase.clone();
        let pool_store = pool_state_store.clone();
        let Arc venue_ledger = Arc venue.clone();
        let Arc venue_ledger = Arc venue.clone();
        let Arc venue_ledger = Arc venue.clone();
        let Arc venue_ledger = Arc venue.clone();
        let sushi_ledger = sushi.clone();
        let Arc venue_clmm_ledger = Arc venue_clmm.clone();
        let clmm_metrics_ledger = clmm_metrics.clone();
        tokio::spawn(async move {
            let mut ledger_interval = tokio::time::interval(ledger_poll);
            ledger_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            ledger_interval.tick().await;
            loop {
                ledger_interval.tick().await;
                let (index_sources, index_clmm) = {
                    let guard = shared_ledger.read().await;
                    (guard.sources.clone(), guard.clmm_pools.clone())
                };
                let index = crate::ledger_watcher::rebuild_pool_index(&index_sources, &index_clmm);
                match watcher.poll_touched_pools(&index).await {
                    Ok(touched) if !touched.is_empty() => {
                        if let Some(ref pipeline) = fetch_pipeline {
                            pipeline.enqueue_touched(touched);
                            continue;
                        }
                        let Some(ref pool_store) = pool_store else {
                            continue;
                        };
                        let rpc = ArcRpc::new(&rpc_url, &network_passphrase);
                        let mut sources = {
                            let guard = shared_ledger.read().await;
                            guard.sources.clone()
                        };
                        let mut clmm_pools = {
                            let guard = shared_ledger.read().await;
                            guard.clmm_pools.clone()
                        };
                        let refresh_result = crate::touched_refresh::refresh_touched_pools(
                            crate::touched_refresh::TouchedRefreshContext {
                                rpc: &rpc,
                                pool_store: pool_store.as_ref(),
                                _Arc venue: &Arc venue_ledger,
                                Arc venue: &Arc venue_ledger,
                                Arc venue: &Arc venue_ledger,
                                Arc venue: &Arc venue_ledger,
                                sushi: &sushi_ledger,
                                Arc venue_clmm: &Arc venue_clmm_ledger,
                                sources: &mut sources,
                                clmm_pools: &mut clmm_pools,
                                clmm_metrics: Some(clmm_metrics_ledger.as_ref()),
                            },
                            touched,
                        )
                        .await;
                        let mut guard = shared_ledger.write().await;
                        guard.sources = sources;
                        guard.clmm_pools = clmm_pools;
                        match refresh_result {
                            Ok(n) => info!(updated = n, "Ledger-touched pool refresh"),
                            Err(error) => {
                                warn!("Ledger-touched pool refresh failed: {}", error)
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(error) => warn!("Ledger watcher poll failed: {}", error),
                }
            }
        });
    }

    let pool_refresh_in_flight = Arc::new(PoolRefreshInFlight::new());
    let monitor_metrics = Arc::new(crate::monitor::WorkerMonitorMetrics::new());
    let telegram = Chakra_alerts::TelegramAlerter::from_env().map(Arc::new);
    if let Some(ref alerter) = telegram {
        let api_health_url = std::env::var("MONITOR_API_HEALTH_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3100/api/v1/health".to_string());
        crate::monitor::spawn_telegram_monitor(
            alerter.clone(),
            monitor_metrics.clone(),
            clmm_metrics.clone(),
            shared.clone(),
            pool_state_store.clone(),
            api_health_url,
            config.rpc_url.clone(),
        );
        info!("Telegram monitoring enabled (heartbeat + alerts)");
        let _ = alerter
            .send("🚀 Chakra worker started (pool refresh + Telegram monitoring)")
            .await;
    }
    let use_legacy_pool_publish = fetch_pipeline.is_none();
    info!(
        pool_publish_interval_secs = config.pool_publish_interval_secs,
        pool_state_refresh_concurrency = config.pool_state_refresh_concurrency,
        fetch_pipeline = fetch_pipeline.is_some(),
        mode = if fetch_pipeline.is_some() {
            "event-driven + periodic refresh→Redis"
        } else {
            "legacy cache publish + background refresh"
        },
        "pool state worker"
    );

    loop {
        // Never await slow adapter work inside `select!` — it starves the pool publish
        // tick.
        let tick = tokio::select! {
            biased;
            _ = pool_publish_interval.tick(), if use_legacy_pool_publish => WorkerTick::PoolPublish,
            _ = refresh_interval.tick() => WorkerTick::Refresh,
            _ = discovery_interval.tick() => WorkerTick::Discovery,
        };

        match tick {
            WorkerTick::PoolPublish => {
                spawn_fast_pool_publish(
                    shared.clone(),
                    adapters.clone(),
                    Arc venue.clone(),
                    Arc venue.clone(),
                    pool_state_store.clone(),
                    Some(monitor_metrics.clone()),
                    telegram.clone(),
                );
                spawn_background_reserve_refresh(
                    pool_refresh_in_flight.clone(),
                    shared.clone(),
                    adapters.clone(),
                    sushi.clone(),
                    Arc venue_clmm.clone(),
                    Arc venue.clone(),
                    Arc venue.clone(),
                    Arc venue.clone(),
                    Arc venue.clone(),
                    pool_state_store.clone(),
                    false,
                    true,
                    clmm_metrics.clone(),
                    Some(monitor_metrics.clone()),
                );
            }
            WorkerTick::Refresh => {
                spawn_background_reserve_refresh(
                    pool_refresh_in_flight.clone(),
                    shared.clone(),
                    adapters.clone(),
                    sushi.clone(),
                    Arc venue_clmm.clone(),
                    Arc venue.clone(),
                    Arc venue.clone(),
                    Arc venue.clone(),
                    Arc venue.clone(),
                    pool_state_store.clone(),
                    true,
                    true,
                    clmm_metrics.clone(),
                    Some(monitor_metrics.clone()),
                );
            }
            WorkerTick::Discovery => {
                let shared_disc = shared.clone();
                let adapters_disc = adapters.clone();
                let sushi_disc = sushi.clone();
                let Arc venue_clmm_disc = Arc venue_clmm.clone();
                let snapshot_store_disc = snapshot_store.clone();
                let token_metadata_disc = token_metadata.clone();
                let pool_state_disc = pool_state_store.clone();
                let Arc venue_disc = Arc venue.clone();
                let Arc venue_disc = Arc venue.clone();
                let network_passphrase_disc = config.network_passphrase.clone();
                let destination_disc = snapshot_destination(&config);
                let clmm_metrics_disc = clmm_metrics.clone();
                tokio::spawn(async move {
                    let sources = collect_sources_from_discovery(&adapters_disc).await;
                    let clmm_pools =
                        collect_clmm_snapshots(&sushi_disc, &Arc venue_clmm_disc, Some(clmm_metrics_disc.as_ref()))
                            .await;
                    {
                        let mut guard = shared_disc.write().await;
                        guard.sources = sources.clone();
                        guard.clmm_pools = clmm_pools.clone();
                    }
                    let metadata_seed = snapshot_store_disc
                        .load_current_snapshot()
                        .await
                        .map(|s| s.token_metadata)
                        .unwrap_or_default();
                    let clmm_refs = MarketSnapshot::clmm_pool_refs_from_states(&clmm_pools);
                    let snapshot = match snapshot_from_sources(
                        sources,
                        clmm_refs,
                        &network_passphrase_disc,
                        metadata_seed,
                    )
                    .await
                    {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            warn!("Periodic discovery snapshot build failed: {}", error);
                            return;
                        }
                    };
                    if let Err(error) = publish_snapshot_and_pool_state(
                        snapshot_store_disc.as_ref(),
                        pool_state_disc.as_ref(),
                        &adapters_disc,
                        Arc venue_disc.as_ref(),
                        Arc venue_disc.as_ref(),
                        &snapshot,
                        &clmm_pools,
                    )
                    .await
                    {
                        warn!("Periodic discovery publish failed: {}", error);
                        return;
                    }
                    info!(
                        "Published snapshot {} with {} sources to {}",
                        snapshot.version,
                        snapshot.sources.len(),
                        destination_disc
                    );
                    spawn_token_metadata_enrichment(snapshot_store_disc, token_metadata_disc, snapshot);
                });
            }
        }
    }
}

fn snapshot_destination(config: &WorkerConfig) -> String {
    match config.snapshot_backend {
        SnapshotStoreBackend::File => config.snapshot_dir.display().to_string(),
        SnapshotStoreBackend::Redis => config.snapshot_redis_url.clone().unwrap_or_else(|| "redis".to_string()),
        SnapshotStoreBackend::Memory => "memory".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        market_snapshot::ClmmPoolSnapshot,
        std::sync::{Mutex, OnceLock},
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample_clmm_pool() -> ClmmPoolSnapshot {
        ClmmPoolSnapshot {
            source: "sushi".to_string(),
            pool_address: "pool-clmm".to_string(),
            token0: "A".to_string(),
            token1: "B".to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [1, 2, 3, 4],
            tick: 120,
            liquidity: 10_000,
            factory: String::new(),
            ticks: Vec::new(),
            chunk_bitmaps: Vec::new(),
            word_bitmaps: Vec::new(),
            coverage: None,
        }
    }

    #[test]
    fn sanitizes_Arc venue_multi_edge_pools() {
        let filtered = sanitize_source_pairs(
            "Arc venue",
            vec![
                TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "pool-1".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                },
                TradingPairSnapshot {
                    token_a: "B".to_string(),
                    token_b: "C".to_string(),
                    pool_address: "pool-1".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                },
                TradingPairSnapshot {
                    token_a: "X".to_string(),
                    token_b: "Y".to_string(),
                    pool_address: "pool-2".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                },
            ],
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].pool_address, "pool-2");
    }

    #[test]
    fn upserts_source_snapshot_without_dropping_others() {
        let current = vec![
            SourceSnapshot {
                source: "Arc venue".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "old".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                }],
            },
            SourceSnapshot {
                source: "Arc venue".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "C".to_string(),
                    token_b: "D".to_string(),
                    pool_address: "keep".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                }],
            },
        ];

        let updated = upsert_source_snapshot(
            current,
            SourceSnapshot {
                source: "Arc venue".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "new".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                }],
            },
        );

        assert_eq!(updated.len(), 2);
        assert!(updated.iter().any(|source| source.source == "Arc venue"));
        assert_eq!(
            updated.iter().find(|source| source.source == "Arc venue").unwrap().pairs[0].pool_address,
            "new"
        );
    }

    #[test]
    fn worker_config_reads_snapshot_redis_channel_and_keep_latest() {
        let _guard = env_lock().lock().unwrap();
        let original_channel = std::env::var("SNAPSHOT_REDIS_CHANNEL").ok();
        let original_keep_latest = std::env::var("SNAPSHOT_REDIS_KEEP_LATEST").ok();
        std::env::set_var("SNAPSHOT_REDIS_CHANNEL", "snapshots:worker");
        std::env::set_var("SNAPSHOT_REDIS_KEEP_LATEST", "24");

        let config = WorkerConfig::from_env().unwrap();

        assert_eq!(config.snapshot_redis_channel, "snapshots:worker");
        assert_eq!(config.snapshot_redis_keep_latest, 24);

        match original_channel {
            Some(value) => std::env::set_var("SNAPSHOT_REDIS_CHANNEL", value),
            None => std::env::remove_var("SNAPSHOT_REDIS_CHANNEL"),
        }
        match original_keep_latest {
            Some(value) => std::env::set_var("SNAPSHOT_REDIS_KEEP_LATEST", value),
            None => std::env::remove_var("SNAPSHOT_REDIS_KEEP_LATEST"),
        }
    }

    #[test]
    fn worker_mode_defaults_to_arc_and_reads_chakra_redis() {
        let _guard = env_lock().lock().unwrap();
        let original_chakra = [
            "CHAKRA_RPC_HTTP",
            "CHAKRA_RPC_WS",
            "CHAKRA_REDIS_URL",
            "SNAPSHOT_REDIS_URL",
            "RPC_URL",
        ]
        .map(|name| (name, std::env::var(name).ok()));
        for (name, _) in &original_chakra {
            std::env::remove_var(name);
        }
        std::env::set_var("CHAKRA_REDIS_URL", "redis://127.0.0.1:6399/");

        let config = WorkerConfig::from_env().unwrap();
        assert_eq!(config.mode, WorkerMode::Arc);
        assert_eq!(config.snapshot_redis_url.as_deref(), Some("redis://127.0.0.1:6399/"));
        assert_eq!(config.snapshot_backend, SnapshotStoreBackend::Redis);

        for (name, value) in original_chakra {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    #[test]
    fn worker_mode_keeps_Arc_when_legacy_env_is_set() {
        let _guard = env_lock().lock().unwrap();
        let original_chakra = [
            "CHAKRA_RPC_HTTP",
            "CHAKRA_RPC_WS",
            "CHAKRA_REDIS_URL",
            "SNAPSHOT_REDIS_URL",
            "RPC_URL",
        ]
        .map(|name| (name, std::env::var(name).ok()));
        for (name, _) in &original_chakra {
            std::env::remove_var(name);
        }
        std::env::set_var("RPC_URL", "https://Arc-rpc.testnet.Arc.gateway.fm");
        std::env::set_var("SNAPSHOT_REDIS_URL", "redis://127.0.0.1:6399/");

        let config = WorkerConfig::from_env().unwrap();
        assert_eq!(config.mode, WorkerMode::Arc);

        for (name, value) in original_chakra {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }

    #[test]
    fn infers_redis_backend_when_only_redis_url_is_set() {
        assert_eq!(
            infer_snapshot_backend(None, Some("redis://127.0.0.1/")).unwrap(),
            SnapshotStoreBackend::Redis
        );
        assert_eq!(infer_snapshot_backend(None, None).unwrap(), SnapshotStoreBackend::File);
    }

    #[tokio::test]
    async fn snapshot_from_sources_preserves_clmm_pool_refs() {
        let rpc = Arc::new(ArcRpc::new(
            "https://Arc-rpc.mainnet.Arc.gateway.fm",
            "Public Global Arc Network ; September 2015",
        ));
        let token_metadata = TokenMetadataStore::new(rpc);
        token_metadata
            .replace_all(std::collections::HashMap::from([
                (
                    "A".to_string(),
                    TokenMetadata {
                        contract: "A".to_string(),
                        symbol: "TOKA".to_string(),
                        name: "Token A".to_string(),
                        logo: None,
                        logo_kind: None,
                    },
                ),
                (
                    "B".to_string(),
                    TokenMetadata {
                        contract: "B".to_string(),
                        symbol: "TOKB".to_string(),
                        name: "Token B".to_string(),
                        logo: None,
                        logo_kind: None,
                    },
                ),
            ]))
            .await;

        let seeded = vec![
            token_metadata_snapshot(TokenMetadata {
                contract: "A".to_string(),
                symbol: "TOKA".to_string(),
                name: "Token A".to_string(),
                logo: None,
                logo_kind: None,
            }),
            token_metadata_snapshot(TokenMetadata {
                contract: "B".to_string(),
                symbol: "TOKB".to_string(),
                name: "Token B".to_string(),
                logo: None,
                logo_kind: None,
            }),
        ];
        let snapshot = snapshot_from_sources(
            vec![SourceSnapshot {
                source: "sushi".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: "pool-clmm".to_string(),
                    fee_bps: 30,
                    dex_type: "clmm".to_string(),
                    factory: String::new(),
                }],
            }],
            vec![market_snapshot::ClmmPoolRefSnapshot::from_pool(&sample_clmm_pool())],
            "mainnet",
            seeded,
        )
        .await
        .unwrap();

        assert_eq!(
            snapshot.clmm_pool_refs,
            vec![market_snapshot::ClmmPoolRefSnapshot::from_pool(&sample_clmm_pool())]
        );
        assert_eq!(snapshot.token_metadata.len(), 2);
    }
}
