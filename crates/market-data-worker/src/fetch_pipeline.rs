//! EVM pool-state fetch pipeline for the Arc path: high-priority `FetchTask`
//! queue → RPC workers → Redis sink.
//!
//! Only EVM venues are handled (`chakra-xyk` / `chakra-stable` /
//! `chakra-clmm` + `discovered:*`). Stellar adapters are never constructed.

use {
    crate::{clmm_metrics::ClmmCoverageMetrics, worker::WorkerShared},
    anyhow::Result,
    dex_adapters::{
        evm_logs::normalize_evm_address,
        evm_rpc::EvmRpcClient,
        pool_index::PoolRef,
    },
    market_snapshot::{
        pool_state_store::{
            should_publish_clmm_to_redis, PoolStateStore, StablePoolStateValue, XykPoolStateValue,
        },
        ClmmPoolRefSnapshot, ClmmPoolSnapshot, TradingPairSnapshot,
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
pub enum FetchTask {
    /// EVM xy=k pool (Arc, chakra-xyk / discovered:xyk).
    EvmXyk {
        pool_address: String,
    },
    /// EVM stableswap pool (Arc, chakra-stable / discovered:stable).
    EvmStable {
        pool_address: String,
    },
    /// EVM CLMM pool (Arc, chakra-clmm / discovered:clmm).
    EvmClmm {
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
            "chakra-xyk" | "discovered:xyk" => tasks.push(FetchTask::EvmXyk {
                pool_address: pool.pool_address,
            }),
            "chakra-stable" | "discovered:stable" => tasks.push(FetchTask::EvmStable {
                pool_address: pool.pool_address,
            }),
            "chakra-clmm" | "discovered:clmm" => tasks.push(FetchTask::EvmClmm {
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
        FetchTask::EvmXyk { pool_address } => {
            let (source, pair) = find_evm_pair(&ctx.shared, "xyk", &pool_address).await?;
            let value = dex_adapters::evm_fetch::fetch_xyk_state(ctx.evm.as_ref(), source.as_str(), &pair).await?;
            Ok(vec![PoolStateUpdate::Xyk(vec![value])])
        }
        FetchTask::EvmStable { pool_address } => {
            let (source, pair) = find_evm_pair(&ctx.shared, "stable", &pool_address).await?;
            let value = dex_adapters::evm_fetch::fetch_stable_state(
                ctx.evm.as_ref(),
                source.as_str(),
                &pair,
                dex_adapters::evm_fetch::CHAKRA_STABLE_A,
            )
            .await?;
            Ok(vec![PoolStateUpdate::Stable(vec![value])])
        }
        FetchTask::EvmClmm { pool_address } => {
            let (pool_ref, existing) = {
                let guard = ctx.shared.read().await;
                let existing = guard
                    .clmm_pools
                    .iter()
                    .find(|p| normalize_evm_address(&p.pool_address) == normalize_evm_address(&pool_address))
                    .cloned();
                let Some(existing) = existing.as_ref() else {
                    anyhow::bail!("EvmClmm pool {pool_address} not in topology");
                };
                (
                    ClmmPoolRefSnapshot {
                        source: existing.source.clone(),
                        pool_address: existing.pool_address.clone(),
                        token0: existing.token0.clone(),
                        token1: existing.token1.clone(),
                        fee_bps: existing.fee_bps,
                        tick_spacing: existing.tick_spacing,
                        factory: existing.factory.clone(),
                    },
                    existing.clone(),
                )
            };
            let snapshot =
                dex_adapters::evm_fetch::fetch_clmm_state(ctx.evm.as_ref(), &pool_ref.source, &pool_ref, Some(&existing))
                    .await?;
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
            let mut guard = ctx.shared.write().await;
            if let Some(current) = guard
                .clmm_pools
                .iter_mut()
                .find(|p| p.pool_address == snapshot.pool_address)
            {
                *current = snapshot.clone();
            } else {
                guard.clmm_pools.push(snapshot.clone());
            }
            Ok(vec![PoolStateUpdate::Clmm(snapshot)])
        }
    }
}

/// Find `(source, pair)` for an EVM xyk/stable pool in the shared topology.
async fn find_evm_pair(
    shared: &tokio::sync::RwLock<WorkerShared>,
    dex_type: &str,
    pool_address: &str,
) -> anyhow::Result<(String, TradingPairSnapshot)> {
    let guard = shared.read().await;
    for source in &guard.sources {
        for pair in &source.pairs {
            if pair.dex_type == dex_type && normalize_evm_address(&pair.pool_address) == normalize_evm_address(pool_address)
            {
                return Ok((source.source.clone(), pair.clone()));
            }
        }
    }
    anyhow::bail!("EVM {dex_type} pool {pool_address} not in topology")
}

#[cfg(test)]
mod tests {
    use super::*;

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
            pref("soroswap", "0xSTELLAR"),
        ]);
        let tasks = coalesce_touched_into_tasks(touched);
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmXyk { pool_address } if pool_address == "0xP1")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmXyk { pool_address } if pool_address == "0xP2")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmStable { pool_address } if pool_address == "0xS1")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmClmm { pool_address } if pool_address == "0xC1")));
        assert!(tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmClmm { pool_address } if pool_address == "0xC2")));
        // Stellar sources never produce tasks on the Arc path.
        assert!(!tasks
            .iter()
            .any(|t| matches!(t, FetchTask::EvmXyk { pool_address } if pool_address == "0xSTELLAR")));
        assert_eq!(tasks.len(), 5);
    }
}
