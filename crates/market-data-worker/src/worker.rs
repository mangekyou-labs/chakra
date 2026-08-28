//! Arc worker config + entry (T3.3+).
//!
//! `run()` always runs `evm_watcher::run_arc`: bootstrap → WS + poll → EVM
//! fetch into `chakra:` Redis keys. Arc adapters are never constructed.

use {
    anyhow::Result,
    market_snapshot::{
        pool_state_store::PoolStateStore,
        store::{
            build_snapshot_store, SnapshotStore, SnapshotStoreBackend, DEFAULT_REDIS_EVENTS_CHANNEL,
            DEFAULT_REDIS_SNAPSHOT_HISTORY,
        },
        ClmmPoolSnapshot, SourceSnapshot, DEFAULT_SNAPSHOT_DIR,
    },
    std::{path::PathBuf, sync::Arc},
    tracing::info,
};

/// Shared graph + CLMM state (main loop and background bootstrap).
pub(crate) struct WorkerShared {
    pub(crate) sources: Vec<SourceSnapshot>,
    pub(crate) clmm_pools: Vec<ClmmPoolSnapshot>,
}

#[derive(Clone)]
pub struct WorkerConfig {
    pub snapshot_backend: SnapshotStoreBackend,
    pub snapshot_dir: PathBuf,
    pub snapshot_redis_url: Option<String>,
    pub snapshot_redis_channel: String,
    pub snapshot_redis_keep_latest: usize,
    /// Concurrent EVM pool-state fetches in the fetch pipeline.
    pub pool_state_refresh_concurrency: usize,
    pub discovery_interval_secs: u64,
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
            .field("snapshot_backend", &self.snapshot_backend)
            .field("snapshot_dir", &self.snapshot_dir)
            .field("snapshot_redis_url", &self.snapshot_redis_url)
            .field("snapshot_redis_channel", &self.snapshot_redis_channel)
            .field("snapshot_redis_keep_latest", &self.snapshot_redis_keep_latest)
            .field("pool_state_refresh_concurrency", &self.pool_state_refresh_concurrency)
            .field("discovery_interval_secs", &self.discovery_interval_secs)
            .field("snapshot_store", &self.snapshot_store.as_ref().map(|_| "<store>"))
            .field("pool_store", &self.pool_store.as_ref().map(|_| "<store>"))
            .finish()
    }
}

impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        // CHAKRA_REDIS_URL is the primary Redis URL; SNAPSHOT_REDIS_URL stays
        // the legacy override.
        let redis_url = std::env::var("SNAPSHOT_REDIS_URL")
            .ok()
            .or_else(|| std::env::var("CHAKRA_REDIS_URL").ok());
        let snapshot_backend =
            infer_snapshot_backend(std::env::var("SNAPSHOT_BACKEND").ok().as_deref(), redis_url.as_deref())?;
        Ok(Self {
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
            pool_state_refresh_concurrency: std::env::var("POOL_STATE_REFRESH_CONCURRENCY")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
            discovery_interval_secs: std::env::var("CHAKRA_DISCOVERY_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(600),
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

/// Arc worker entry: bootstrap + WS/poll + EVM fetch into Redis. Never returns.
pub async fn run(config: WorkerConfig) -> Result<()> {
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
    info!(
        snapshot_backend = ?config.snapshot_backend,
        "Chakra market-data-worker starting (Arc watcher)"
    );
    crate::evm_watcher::run_arc(config).await
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        market_snapshot::store::SnapshotStoreBackend,
        std::sync::{Mutex, OnceLock},
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
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
    fn worker_config_reads_chakra_redis_and_defaults_to_redis_backend() {
        let _guard = env_lock().lock().unwrap();
        let original_chakra =
            ["CHAKRA_REDIS_URL", "SNAPSHOT_REDIS_URL", "SNAPSHOT_BACKEND"].map(|name| (name, std::env::var(name).ok()));
        for (name, _) in &original_chakra {
            std::env::remove_var(name);
        }
        std::env::set_var("CHAKRA_REDIS_URL", "redis://127.0.0.1:6399/");

        let config = WorkerConfig::from_env().unwrap();
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
    fn snapshot_redis_url_overrides_chakra_redis_url() {
        let _guard = env_lock().lock().unwrap();
        let original_chakra = ["CHAKRA_REDIS_URL", "SNAPSHOT_REDIS_URL"].map(|name| (name, std::env::var(name).ok()));
        std::env::set_var("CHAKRA_REDIS_URL", "redis://chakra:6379");
        std::env::set_var("SNAPSHOT_REDIS_URL", "redis://snapshot:6379");

        let config = WorkerConfig::from_env().unwrap();
        assert_eq!(
            config.snapshot_redis_url.as_deref(),
            Some("redis://snapshot:6379"),
            "legacy SNAPSHOT_REDIS_URL must win (override)"
        );

        for (name, value) in original_chakra {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
