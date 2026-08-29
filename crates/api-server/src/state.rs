//! Shared application state (Chakra surface).
//!
//! The loaded-engine slot is an atomic `Option<(Option<String>, Arc<QuoteEngine>)>`
//! protected by a single-flight async mutex. Per-request version checks happen in
//! the `/quote` handler — if the snapshot version pointer changed, the handler
//! acquires the reload mutex, rebuilds the engine from a fresh snapshot, and
//! swaps the slot only after a successful construction.

use {
    crate::{config::AppConfig, snapshot_loader},
    market_snapshot::{
        pool_state_store::{MemoryPoolStateStore, PoolStateStore, RedisPoolStateStore},
        store::{MemorySnapshotStore, SnapshotStore},
    },
    std::sync::Arc,
    tokio::sync::{Mutex, RwLock},
};

/// Loaded engine slot: (version pointer, built engine).
#[derive(Clone)]
struct LoadedEngine {
    version: Option<String>,
    engine: Arc<router_engine::QuoteEngine>,
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Atomic loaded-engine slot — swapped atomically (version + engine together).
    loaded_engine: Arc<RwLock<Option<LoadedEngine>>>,
    /// Single-flight reload mutex — only one concurrent reload attempt.
    reload_mutex: Arc<Mutex<()>>,
    pub config: AppConfig,
    /// Per-pool state (Redis `chakra:pool:*` or in-memory).
    pub pool_store: Option<Arc<dyn PoolStateStore>>,
    /// Cluster snapshot store (Redis) — loaded at startup, used by `/build_tx`.
    pub snapshot_store: Option<Arc<dyn market_snapshot::store::SnapshotStore>>,
    /// Embedded snapshot store (memory backend).
    pub memory_snapshot: Option<Arc<MemorySnapshotStore>>,
    /// Embedded pool store (memory backend).
    pub memory_pool: Option<Arc<MemoryPoolStateStore>>,
    /// Chakra EVM RPC (never contacted by `/quote`; used by `/balances` and
    /// `/build_tx`).
    pub evm_rpc: Option<Arc<dex_adapters::evm_rpc::EvmRpcClient>>,
}

impl AppState {
    /// Chakra cluster state from env (Redis read-only for quotes).
    pub async fn from_env() -> anyhow::Result<Self> {
        let config = AppConfig::from_env();
        let redis_url = config.snapshot_redis_url.clone();
        let pool_store: Option<Arc<dyn PoolStateStore>> = match redis_url.as_deref() {
            Some(url) => Some(Arc::new(RedisPoolStateStore::with_default_ttl(url)?)),
            None => None,
        };
        let evm_rpc = Some(Arc::new(dex_adapters::evm_rpc::EvmRpcClient::single(
            &config.chakra_rpc_http,
        )?));

        // Build an initial empty engine.
        // Cluster mode: try to load snapshot from Redis at startup.
        let snapshot_store: Option<Arc<dyn market_snapshot::store::SnapshotStore>> = match redis_url.as_deref() {
            Some(url) => Some(Arc::new(market_snapshot::store::RedisSnapshotStore::new(url))),
            None => None,
        };

        let loaded_engine = if let Some(store) = &snapshot_store {
            match store.load_current_snapshot().await {
                Ok(snapshot) => {
                    let version = snapshot.version.clone();
                    match snapshot_loader::build_engine_from_snapshot(&config, &snapshot).await {
                        Ok(engine) => {
                            tracing::info!("startup snapshot loaded: version={version}");
                            Some(LoadedEngine {
                                version: Some(version),
                                engine: Arc::new(engine),
                            })
                        }
                        Err(e) => {
                            tracing::warn!("startup engine build failed: {e}");
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("startup snapshot load skipped: {e}");
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            loaded_engine: Arc::new(RwLock::new(loaded_engine)),
            reload_mutex: Arc::new(Mutex::new(())),
            config,
            pool_store,
            snapshot_store,
            memory_snapshot: None,
            memory_pool: None,
            evm_rpc,
        })
    }

    /// Explicit constructor for tests and harnesses. Provides private slot
    /// initialization without exposing struct literals.
    /// `pre_loaded_engine` lets tests skip the snapshot-store rebuild step by
    /// injecting an already-built engine + version pair directly.
    pub async fn from_backends(
        config: AppConfig,
        snapshot_store: Option<Arc<dyn SnapshotStore>>,
        pool_store: Option<Arc<dyn PoolStateStore>>,
        memory_snapshot: Option<Arc<MemorySnapshotStore>>,
        memory_pool: Option<Arc<MemoryPoolStateStore>>,
        evm_rpc: Option<Arc<dex_adapters::evm_rpc::EvmRpcClient>>,
        pre_loaded_engine: Option<(String, Arc<router_engine::QuoteEngine>)>,
    ) -> Self {
        Self {
            loaded_engine: Arc::new(RwLock::new(pre_loaded_engine.map(|(v, e)| LoadedEngine {
                version: Some(v),
                engine: e,
            }))),
            reload_mutex: Arc::new(Mutex::new(())),
            config,
            pool_store,
            snapshot_store,
            memory_snapshot,
            memory_pool,
            evm_rpc,
        }
    }

    /// Return a snapshot of the current engine version (if loaded).
    pub async fn loaded_version(&self) -> Option<String> {
        self.loaded_engine
            .read()
            .await
            .as_ref()
            .map(|e| e.version.clone().unwrap_or_default())
    }

    /// Return a clone of the current engine. Used by `/build_tx`.
    pub async fn current_engine(&self) -> Arc<router_engine::QuoteEngine> {
        self.loaded_engine
            .read()
            .await
            .as_ref()
            .map(|e| e.engine.clone())
            .unwrap_or_else(|| {
                Arc::new(router_engine::QuoteEngine::new(
                    router_engine::path_finder::PathFinderConfig::default(),
                    router_engine::split_optimizer::SplitConfig::default(),
                ))
            })
    }

    /// Readiness: report loaded engine version when available.
    /// A stale-but-usable engine remains ready.
    pub async fn ready(&self) -> Option<(String, Vec<String>)> {
        // Embedded mode: memory snapshot + memory pool store.
        if let Some(snapshots) = &self.memory_snapshot {
            let pools = self.memory_pool.as_ref()?;
            if !market_snapshot::ready::memory_ready(snapshots, pools).await {
                return None;
            }
            let snapshot = snapshots.load_current_snapshot().await.ok()?;
            let mut keys: Vec<String> = Vec::new();
            for value in pools.pool_keys().await {
                keys.push(value);
            }
            return Some((snapshot.version.clone(), keys));
        }

        // Cluster mode: check Redis pool store + loaded engine.
        if let Some(_store) = &self.pool_store {
            let url = self
                .config
                .snapshot_redis_url
                .as_deref()
                .unwrap_or("redis://127.0.0.1:6379");
            if !market_snapshot::ready::cluster_ready(url).await.unwrap_or(false) {
                return None;
            }
            let loaded = self.loaded_engine.read().await;
            if let Some(le) = loaded.as_ref() {
                let edges = le.engine.cached_pool_edges().await;
                if edges.is_empty() {
                    return None;
                }
                let version = le.version.clone().unwrap_or_default();
                return Some((version, Vec::new()));
            }
            return None;
        }

        None
    }

    /// Try to acquire a usable engine for a given version pointer.
    /// If versions match, returns immediately. On mismatch, acquires the reload
    /// mutex, rechecks the pointer, loads a fresh snapshot, builds a new engine,
    /// and swaps the slot on success.
    pub async fn engine_for_version(
        &self,
        pointer_version: &str,
    ) -> Result<Arc<router_engine::QuoteEngine>, EngineError> {
        // Fast path: version matches.
        {
            let loaded = self.loaded_engine.read().await;
            if let Some(le) = loaded.as_ref() {
                if le.version.as_deref() == Some(pointer_version) {
                    return Ok(le.engine.clone());
                }
            }
        }

        // Slow path: acquire reload mutex.
        let _guard = self.reload_mutex.lock().await;

        // Recheck after acquiring lock (another request may have reloaded).
        {
            let loaded = self.loaded_engine.read().await;
            if let Some(le) = loaded.as_ref() {
                if le.version.as_deref() == Some(pointer_version) {
                    return Ok(le.engine.clone());
                }
            }
        }

        // Reload the full snapshot and rebuild the engine.
        let store = self.snapshot_store.as_ref().ok_or(EngineError::NoStore)?;
        let snapshot = store.load_current_snapshot().await.map_err(EngineError::SnapshotLoad)?;
        let new_engine = snapshot_loader::build_engine_from_snapshot(&self.config, &snapshot)
            .await
            .map_err(EngineError::EngineBuild)?;

        // Verify the topology is non-empty.
        if new_engine.cached_pool_edges().await.is_empty() {
            return Err(EngineError::EmptyTopology);
        }

        let version = snapshot.version.clone();
        let engine = Arc::new(new_engine);

        // Swap in the new engine + version.
        {
            let mut loaded = self.loaded_engine.write().await;
            *loaded = Some(LoadedEngine {
                version: Some(version),
                engine: engine.clone(),
            });
        }

        Ok(engine)
    }

    /// Return the best-effort engine: either a version-matched one or the last
    /// usable engine (even if stale). Returns None only on cold start with no
    /// engine loaded.
    pub async fn best_effort_engine(&self) -> Option<Arc<router_engine::QuoteEngine>> {
        let loaded = self.loaded_engine.read().await;
        loaded.as_ref().map(|le| le.engine.clone())
    }
}

pub(crate) fn sanitize_cached_pairs(
    source: &str,
    pairs: Vec<router_engine::TradingPair>,
) -> Vec<router_engine::TradingPair> {
    if source != "Arc venue" {
        return pairs;
    }

    let mut by_pool: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for pair in &pairs {
        *by_pool.entry(pair.pool_address.clone()).or_insert(0) += 1;
    }

    // Arc venue multi-token pools are represented as multiple edges sharing one pool
    // address. Those routes are not executable by the current on-chain
    // aggregator, so never hydrate them from disk cache during startup.
    pairs
        .into_iter()
        .filter(|pair| by_pool.get(&pair.pool_address).copied().unwrap_or(0) == 1)
        .collect()
}

#[derive(Debug)]
pub enum EngineError {
    NoStore,
    SnapshotLoad(anyhow::Error),
    EngineBuild(anyhow::Error),
    EmptyTopology,
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoStore => write!(f, "no snapshot store configured"),
            Self::SnapshotLoad(e) => write!(f, "snapshot load failed: {e}"),
            Self::EngineBuild(e) => write!(f, "engine build failed: {e}"),
            Self::EmptyTopology => write!(f, "snapshot produced empty topology"),
        }
    }
}
