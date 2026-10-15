//! EVM pool-state fetch pipeline for the Arc path: high-priority `FetchTask`
//! queue → RPC workers → Redis sink.
//!
/// EVM venues are handled (`chakra-xyk` / `chakra-stable` /
/// `chakra-clmm` / `presto-hub` + `discovered:*`).
use {
    crate::{clmm_metrics::ClmmCoverageMetrics, worker::WorkerShared},
    anyhow::Result,
    dex_adapters::{evm_logs::normalize_evm_address, evm_rpc::EvmRpcClient, pool_index::PoolRef},
    market_snapshot::{
        pool_state_store::{should_publish_clmm_to_redis, PoolStateStore, StablePoolStateValue, XykPoolStateValue},
        ClmmPoolSnapshot, TradingPairSnapshot,
    },
    std::{
        collections::HashSet,
        sync::{
            atomic::{AtomicU64, AtomicUsize, Ordering},
            Arc,
        },
    },
    tokio::sync::{mpsc, RwLock},
    tracing::{debug, info, warn},
};

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum FetchTask {
    /// EVM xy=k pool (Arc, chakra-xyk / discovered:xyk).
    EvmXyk {
        source: String,
        dex_type: String,
        pool_address: String,
    },
    /// EVM stableswap pool (Arc, chakra-stable / discovered:stable).
    EvmStable {
        source: String,
        dex_type: String,
        pool_address: String,
    },
    /// EVM CLMM pool (Arc, chakra-clmm / discovered:clmm).
    EvmClmm {
        source: String,
        dex_type: String,
        pool_address: String,
    },
    /// XyloNet stableswap pool (T-XYLO, source `xylo`). Catalog USDC/EURC
    /// only; the topology snapshot carries the pinned pool.
    EvmXylo {
        source: String,
        dex_type: String,
        pool_address: String,
    },
    /// Presto hub spoke (2026-08-29, source `presto-hub`).
    EvmPresto {
        source: String,
        dex_type: String,
        pool_address: String,
    },
}

#[derive(Default)]
pub struct FetchPipelineMetrics {
    pub high_dropped: AtomicU64,
    pub tasks_completed: AtomicU64,
    pub tasks_failed: AtomicU64,
    pub redis_writes: AtomicU64,
    high_depth: AtomicUsize,
}

impl FetchPipelineMetrics {
    fn log_periodic_summary(&self, clmm: Option<&ClmmCoverageMetrics>) {
        if let Some(clmm) = clmm {
            let snap = clmm.snapshot();
            info!(
                high_dropped = self.high_dropped.load(Ordering::Relaxed),
                tasks_completed = self.tasks_completed.load(Ordering::Relaxed),
                tasks_failed = self.tasks_failed.load(Ordering::Relaxed),
                redis_writes = self.redis_writes.load(Ordering::Relaxed),
                high_queue_depth = self.high_depth.load(Ordering::Relaxed),
                clmm_refresh_attempts = snap.refresh_attempts,
                clmm_publish_skipped_incomplete = snap.publish_skipped_incomplete,
                clmm_published_complete = snap.published_complete,
                clmm_skip_rate_bps = ClmmCoverageMetrics::skip_rate_bps(snap),
                "fetch pipeline stats"
            );
        } else {
            info!(
                high_dropped = self.high_dropped.load(Ordering::Relaxed),
                tasks_completed = self.tasks_completed.load(Ordering::Relaxed),
                tasks_failed = self.tasks_failed.load(Ordering::Relaxed),
                redis_writes = self.redis_writes.load(Ordering::Relaxed),
                high_queue_depth = self.high_depth.load(Ordering::Relaxed),
                "fetch pipeline stats"
            );
        }
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum PoolStateUpdate {
    Xyk(Vec<XykPoolStateValue>),
    Stable(Vec<StablePoolStateValue>),
    Clmm(ClmmPoolSnapshot),
}

pub struct FetchPipelineConfig {
    pub worker_count: usize,
    pub refresh_concurrency: usize,
    pub high_queue_capacity: usize,
}

impl FetchPipelineConfig {
    pub fn from_env(refresh_concurrency: usize) -> Self {
        Self {
            worker_count: std::env::var("FETCH_WORKER_COUNT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8)
                .max(1),
            refresh_concurrency,
            high_queue_capacity: std::env::var("FETCH_HIGH_QUEUE_CAPACITY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(512)
                .max(64),
        }
    }
}

#[derive(Clone)]
pub struct FetchPipelineHandle {
    high_tx: mpsc::Sender<FetchTask>,
    metrics: Arc<FetchPipelineMetrics>,
}

impl FetchPipelineHandle {
    pub fn enqueue_touched(&self, touched: HashSet<PoolRef>) {
        for task in coalesce_touched_into_tasks(touched) {
            match self.high_tx.try_send(task) {
                Ok(()) => {
                    self.metrics.high_depth.fetch_add(1, Ordering::Relaxed);
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.metrics.high_dropped.fetch_add(1, Ordering::Relaxed);
                    warn!("fetch pipeline high queue full, dropping ledger task");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    warn!("fetch pipeline high queue closed");
                }
            }
        }
    }
}

/// One task per touched pool (EVM fetches are single-pool RPC calls).
pub(crate) fn coalesce_touched_into_tasks(touched: HashSet<PoolRef>) -> Vec<FetchTask> {
    let mut tasks = Vec::new();
    for pool in touched {
        match pool.source.as_str() {
            // Arc venues (seed = chakra-*, discovered factories = discovered:*).
            "chakra-xyk" | "discovered:xyk" | "unitflow-v25" => tasks.push(FetchTask::EvmXyk {
                source: pool.source,
                dex_type: "xyk".into(),
                pool_address: pool.pool_address,
            }),
            "chakra-stable" | "discovered:stable" => tasks.push(FetchTask::EvmStable {
                source: pool.source,
                dex_type: "stable".into(),
                pool_address: pool.pool_address,
            }),
            "chakra-clmm" | "discovered:clmm" => tasks.push(FetchTask::EvmClmm {
                source: pool.source,
                dex_type: "clmm".into(),
                pool_address: pool.pool_address,
            }),
            // T-XYLO: the pinned XyloNet USDC/EURC pool (source `xylo-stable`).
            "xylo" | "xylo-stable" | "discovered:xylo" | "chakra-xylo" => tasks.push(FetchTask::EvmXylo {
                source: pool.source,
                dex_type: "xylo".into(),
                pool_address: pool.pool_address,
            }),
            // Presto hub spokes are fetched as stable-family state (2026-08-29).
            "presto-hub" | "discovered:presto" => tasks.push(FetchTask::EvmPresto {
                source: pool.source,
                dex_type: "presto".into(),
                pool_address: pool.pool_address,
            }),
            other => {
                debug!(source = other, pool = %pool.pool_address, "ledger touch: no fetch handler");
            }
        }
    }
    tasks
}

struct FetchWorkerContext {
    evm: Arc<EvmRpcClient>,
    shared: Arc<RwLock<WorkerShared>>,
    clmm_metrics: Option<Arc<ClmmCoverageMetrics>>,
}

pub fn spawn_fetch_pipeline(
    config: FetchPipelineConfig,
    pool_store: Arc<dyn PoolStateStore>,
    evm: Arc<EvmRpcClient>,
    shared: Arc<RwLock<WorkerShared>>,
) -> FetchPipelineHandle {
    let (high_tx, mut high_rx) = mpsc::channel::<FetchTask>(config.high_queue_capacity);
    let (redis_tx, mut redis_rx) = mpsc::channel(config.high_queue_capacity.max(1024));

    let metrics = Arc::new(FetchPipelineMetrics::default());
    let stats_metrics = metrics.clone();

    let worker_ctx = Arc::new(FetchWorkerContext {
        evm,
        shared,
        clmm_metrics: None,
    });

    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.worker_count));
    let dispatch_ctx = worker_ctx.clone();
    let dispatch_redis = redis_tx.clone();
    let dispatch_metrics = metrics.clone();
    tokio::spawn(async move {
        while let Some(task) = high_rx.recv().await {
            dispatch_metrics.high_depth.fetch_sub(1, Ordering::Relaxed);

            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let ctx = dispatch_ctx.clone();
            let redis_tx = dispatch_redis.clone();
            let dispatch_metrics = dispatch_metrics.clone();
            tokio::spawn(async move {
                let _permit = permit;
                match execute_fetch_task(ctx.as_ref(), task).await {
                    Ok(updates) => {
                        for update in updates {
                            if redis_tx.send(update).await.is_err() {
                                return;
                            }
                        }
                        dispatch_metrics.tasks_completed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => {
                        dispatch_metrics.tasks_failed.fetch_add(1, Ordering::Relaxed);
                        warn!(%error, "fetch task failed");
                    }
                }
            });
        }
    });

    let pool_store_sink = pool_store.clone();
    let redis_metrics = metrics.clone();
    tokio::spawn(async move {
        while let Some(update) = redis_rx.recv().await {
            let result = match update {
                PoolStateUpdate::Xyk(values) if !values.is_empty() => pool_store_sink.set_xyk_batch(&values).await,
                PoolStateUpdate::Stable(values) if !values.is_empty() => {
                    pool_store_sink.set_stable_batch(&values).await
                }
                PoolStateUpdate::Clmm(snapshot) => pool_store_sink.set_clmm_batch(&[snapshot]).await,
                _ => Ok(()),
            };
            if let Err(error) = result {
                warn!(%error, "redis pool state write failed");
            } else {
                redis_metrics.redis_writes.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    let stats_interval_secs = std::env::var("FETCH_STATS_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
        .max(15);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(stats_interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            interval.tick().await;
            stats_metrics.log_periodic_summary(None);
        }
    });

    info!(
        worker_count = config.worker_count,
        refresh_concurrency = config.refresh_concurrency,
        "Fetch pipeline started (ledger touched → EVM RPC workers → Redis)"
    );

    FetchPipelineHandle { high_tx, metrics }
}

async fn execute_fetch_task(ctx: &FetchWorkerContext, task: FetchTask) -> Result<Vec<PoolStateUpdate>> {
    match task {
        FetchTask::EvmXyk {
            source,
            dex_type,
            pool_address,
        } => {
            debug_assert_eq!(dex_type, "xyk");
            let pair = find_evm_pair_exact(&ctx.shared, &source, "xyk", &pool_address).await?;
            let value = dex_adapters::evm_fetch::fetch_xyk_state(ctx.evm.as_ref(), source.as_str(), &pair).await?;
            Ok(vec![PoolStateUpdate::Xyk(vec![value])])
        }
        FetchTask::EvmStable {
            source,
            dex_type,
            pool_address,
        } => {
            debug_assert!(dex_type == "stable" || dex_type == "presto");
            let pair = find_evm_pair_exact(&ctx.shared, &source, "stable", &pool_address).await?;
            let value = dex_adapters::evm_fetch::fetch_stable_state(
                ctx.evm.as_ref(),
                source.as_str(),
                &pair,
                dex_adapters::evm_fetch::CHAKRA_STABLE_A,
            )
            .await?;
            Ok(vec![PoolStateUpdate::Stable(vec![value])])
        }
        FetchTask::EvmXylo {
            source,
            dex_type,
            pool_address,
        } => {
            debug_assert_eq!(dex_type, "xylo");
            let pair = find_evm_pair_exact(&ctx.shared, &source, "xylo", &pool_address).await?;
            let value = dex_adapters::evm_fetch::fetch_xylo_state(ctx.evm.as_ref(), source.as_str(), &pair).await?;
            Ok(vec![PoolStateUpdate::Stable(vec![value])])
        }
        FetchTask::EvmPresto {
            source,
            dex_type,
            pool_address,
        } => {
            debug_assert_eq!(dex_type, "presto");
            let pair = find_evm_pair_exact(&ctx.shared, &source, "presto", &pool_address).await?;
            let value = dex_adapters::evm_fetch::fetch_presto_state(ctx.evm.as_ref(), source.as_str(), &pair).await?;
            Ok(vec![PoolStateUpdate::Stable(vec![value])])
        }
        FetchTask::EvmClmm {
            source,
            dex_type,
            pool_address,
        } => {
            debug_assert_eq!(dex_type, "clmm");
            let refresh = ctx.shared.read().await.begin_clmm_refresh(&source, &pool_address)?;
            let pool_ref = &refresh.pool;
            let snapshot =
                dex_adapters::evm_fetch::fetch_clmm_snapshot(ctx.evm.as_ref(), &pool_ref.source, pool_ref).await?;
            if let Some(metrics) = &ctx.clmm_metrics {
                metrics.record_snapshot(&snapshot);
            }
            if !should_publish_clmm_to_redis(&snapshot) {
                debug!(
                    pool = %pool_address,
                    "EvmClmm fetch: skipped Redis publish (incomplete coverage)"
                );
                return Ok(vec![]);
            }
            let accepted = ctx.shared.write().await.accept_clmm_refresh(refresh, snapshot);
            Ok(accepted.into_iter().map(PoolStateUpdate::Clmm).collect())
        }
    }
}

/// Find `(source, pair)` for an EVM xyk/stable pool in the shared topology.
async fn find_evm_pair_exact(
    shared: &tokio::sync::RwLock<WorkerShared>,
    source_id: &str,
    dex_type: &str,
    pool_address: &str,
) -> anyhow::Result<TradingPairSnapshot> {
    let guard = shared.read().await;
    for source in guard.sources() {
        if source.source != source_id {
            continue;
        }
        for pair in &source.pairs {
            if (pair.dex_type == dex_type || (dex_type == "stable" && pair.dex_type == "presto"))
                && normalize_evm_address(&pair.pool_address) == normalize_evm_address(pool_address)
            {
                return Ok(pair.clone());
            }
        }
    }
    anyhow::bail!("EVM {dex_type} pool {pool_address} source {source_id} not in topology")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clmm_rpc_completion_after_discovery_replacement_is_not_forwarded() {
        for remove in [true, false] {
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let started_tx = std::sync::Mutex::new(Some(started_tx));
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let release_rx = std::sync::Mutex::new(release_rx);
            let (url, _server) = crate::evm_watcher::tests::spawn_fixture_rpc(move |method, params| {
                if method == "eth_blockNumber" {
                    if let Some(tx) = started_tx.lock().unwrap().take() {
                        tx.send(()).unwrap();
                        release_rx
                            .lock()
                            .unwrap()
                            .recv_timeout(std::time::Duration::from_secs(5))
                            .unwrap();
                    }
                    return Ok(serde_json::json!("0x42"));
                }
                assert_eq!(method, "eth_call");
                let data = params[0]["data"].as_str().unwrap();
                let value = if data == "0x3850c7bd" {
                    format!("0x{:064x}{:064x}", 1u128 << 96, 0)
                } else if data == "0x1a686502" {
                    format!("0x{:064x}", 1_000_000)
                } else if data == "0xd0c93a7c" {
                    format!("0x{:064x}", 10)
                } else {
                    format!("0x{:064x}", 0)
                };
                Ok(serde_json::json!(value))
            });
            let pool = ClmmPoolSnapshot {
                source: "chakra-clmm".into(),
                pool_address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                token0: "a".into(),
                token1: "b".into(),
                fee_bps: 5,
                tick_spacing: 10,
                factory: "factory".into(),
                sqrt_price_x96: [0; 4],
                tick: 0,
                liquidity: 0,
                ticks: vec![],
                chunk_bitmaps: vec![],
                word_bitmaps: vec![],
                coverage: None,
            };
            let shared = Arc::new(RwLock::new(WorkerShared::default()));
            shared.write().await.replace(vec![], vec![pool.clone()]).await;
            let ctx = FetchWorkerContext {
                evm: Arc::new(EvmRpcClient::single(&url).unwrap()),
                shared: shared.clone(),
                clmm_metrics: None,
            };
            let address = pool.pool_address.clone();
            let task = tokio::spawn(async move {
                execute_fetch_task(
                    &ctx,
                    FetchTask::EvmClmm {
                        source: "chakra-clmm".into(),
                        dex_type: "clmm".into(),
                        pool_address: address,
                    },
                )
                .await
            });
            tokio::time::timeout(std::time::Duration::from_secs(5), started_rx)
                .await
                .unwrap()
                .unwrap();
            shared
                .write()
                .await
                .replace(vec![], if remove { vec![] } else { vec![pool] })
                .await;
            release_tx.send(()).unwrap();
            let updates = task.await.unwrap().unwrap();
            assert!(updates.is_empty(), "superseded refresh must not reach the Redis sink");
            let current = shared.read().await;
            assert_eq!(current.clmm_pools().len(), if remove { 0 } else { 1 });
            assert!(current.clmm_pools().iter().all(|p| p.liquidity == 0));
        }
    }

    fn pref(source: &str, pool: &str) -> PoolRef {
        PoolRef {
            source: source.to_string(),
            pool_address: pool.to_string(),
        }
    }

    #[test]
    fn coalesce_maps_evm_chakra_sources_to_evm_tasks() {
        let touched = HashSet::from([
            pref("chakra-xyk", "0xP1"),
            pref("discovered:xyk", "0xP2"),
            pref("chakra-stable", "0xS1"),
            pref("discovered:clmm", "0xC1"),
            pref("chakra-clmm", "0xC2"),
            pref("presto-hub", "0xPRESTO"),
            pref("unknown-venue", "0xUNKNOWN"),
        ]);
        let tasks = coalesce_touched_into_tasks(touched);
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmXyk { pool_address, .. } if pool_address == "0xP1")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmXyk { pool_address, .. } if pool_address == "0xP2")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmStable { pool_address, .. } if pool_address == "0xS1")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmClmm { pool_address, .. } if pool_address == "0xC1")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmClmm { pool_address, .. } if pool_address == "0xC2")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmPresto { pool_address, .. } if pool_address == "0xPRESTO")));
        // Unknown sources never produce tasks on the Arc path.
        assert!(!tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmXyk { pool_address, .. } if pool_address == "0xUNKNOWN")));
        assert_eq!(tasks.len(), 6);
    }

    #[tokio::test]
    async fn execute_fetch_task_hydrates_presto_pool_reserves_with_distinct_getters() {
        use {crate::evm_watcher::tests::spawn_fixture_rpc, market_snapshot::SourceSnapshot, serde_json::json};
        let path_sel = dex_adapters::evm_fetch::path_reserves_selector();
        let token_sel = dex_adapters::evm_fetch::token_reserves_selector();
        let balance_of_sel = dex_adapters::evm_fetch::balance_of_selector();

        let (url, _server) = spawn_fixture_rpc(move |method, params| {
            if method == "eth_call" {
                let to = params[0]["to"].as_str().unwrap_or("");
                let data = params[0]["data"].as_str().unwrap_or("");
                if data.starts_with(&balance_of_sel) {
                    return Err(json!("balanceOf must not be called on Presto hub"));
                }
                if to.eq_ignore_ascii_case("0x5794a8284A29493871Fbfa3c4f343D42001424D6")
                    || to.eq_ignore_ascii_case("0x5794a8284A29493871Fbfa3c4f343D42001424D7")
                {
                    if data.starts_with(&path_sel) {
                        return Ok(json!(format!("0x{:0>64x}", 123_000_000_000u128)));
                    }
                    if data.starts_with(&token_sel) {
                        return Ok(json!(format!("0x{:0>64x}", 456_000_000_000u128)));
                    }
                }
            }
            Ok(json!("0x0"))
        });
        let client = Arc::new(EvmRpcClient::single(&url).unwrap());
        let shared = Arc::new(RwLock::new(WorkerShared::new(
            vec![SourceSnapshot {
                source: "presto-hub".to_string(),
                pairs: vec![
                    TradingPairSnapshot {
                        token_a: "0x3600000000000000000000000000000000000000".to_string(),
                        token_b: "0xbef5f6d51cb62b58e6a8f77868681825c6fe21c1".to_string(),
                        pool_address: "0x5794a8284A29493871Fbfa3c4f343D42001424D6".to_string(),
                        fee_bps: 30,
                        dex_type: "presto".to_string(),
                        factory: "0x5794a8284A29493871Fbfa3c4f343D42001424D6".to_string(),
                    },
                    TradingPairSnapshot {
                        token_a: "0xbef5f6d51cb62b58e6a8f77868681825c6fe21c1".to_string(),
                        token_b: "0x3600000000000000000000000000000000000000".to_string(),
                        pool_address: "0x5794a8284A29493871Fbfa3c4f343D42001424D7".to_string(),
                        fee_bps: 30,
                        dex_type: "presto".to_string(),
                        factory: "0x5794a8284A29493871Fbfa3c4f343D42001424D6".to_string(),
                    },
                ],
            }],
            Vec::new(),
        )));
        let ctx = FetchWorkerContext {
            evm: client,
            shared,
            clmm_metrics: None,
        };

        // Forward direction: token_a is USDC (path), token_b is EURC (spoke)
        let updates = execute_fetch_task(
            &ctx,
            FetchTask::EvmPresto {
                source: "presto-hub".to_string(),
                dex_type: "presto".to_string(),
                pool_address: "0x5794a8284A29493871Fbfa3c4f343D42001424D6".to_string(),
            },
        )
        .await
        .unwrap();
        assert_eq!(updates.len(), 1);
        let PoolStateUpdate::Stable(states) = &updates[0] else {
            panic!("expected PoolStateUpdate::Stable");
        };
        let state = &states[0];
        assert_eq!(state.source, "presto-hub");
        assert_eq!(
            state.balance_a, 123_000_000_000,
            "balance_a must be pathReserves (USDC)"
        );
        assert_eq!(
            state.balance_b, 456_000_000_000,
            "balance_b must be tokenReserves (EURC)"
        );
        assert_eq!(state.a, 1);
        assert_eq!(state.fee_bps, 30);

        // Reverse direction: token_a is EURC (spoke), token_b is USDC (path)
        let updates_rev = execute_fetch_task(
            &ctx,
            FetchTask::EvmPresto {
                source: "presto-hub".to_string(),
                dex_type: "presto".to_string(),
                pool_address: "0x5794a8284A29493871Fbfa3c4f343D42001424D7".to_string(),
            },
        )
        .await
        .unwrap();
        assert_eq!(updates_rev.len(), 1);
        let PoolStateUpdate::Stable(states_rev) = &updates_rev[0] else {
            panic!("expected PoolStateUpdate::Stable");
        };
        let state_rev = &states_rev[0];
        assert_eq!(state_rev.source, "presto-hub");
        assert_eq!(
            state_rev.balance_a, 456_000_000_000,
            "balance_a must be tokenReserves (EURC)"
        );
        assert_eq!(
            state_rev.balance_b, 123_000_000_000,
            "balance_b must be pathReserves (USDC)"
        );
        assert_eq!(state_rev.a, 1);
        assert_eq!(state_rev.fee_bps, 30);
    }
}
