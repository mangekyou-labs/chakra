//! Application configuration loaded from environment variables or config file.

use serde::{Deserialize, Serialize};

/// Parse + validate the Chakra HTTP RPC URL (T4.3.8 RPC policy).
/// Public Arc + documented failovers only; Canteen `$RPC` and invented
/// Alchemy URLs fail at config load.
pub fn parse_chakra_rpc_http(value: Option<String>) -> anyhow::Result<String> {
    let value = value.unwrap_or_else(|| dex_adapters::evm_rpc::ARC_RPC_HTTP.to_string());
    dex_adapters::evm_rpc::validate_http_urls(std::slice::from_ref(&value))?;
    Ok(value)
}

/// Deployment topology for quote + market data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    /// Separate `market-data-worker` + Redis + API (production default).
    #[default]
    Cluster,
    /// Single process: embedded worker + in-memory stores (self-host /
    /// Jupiter-like).
    Embedded,
}

impl RuntimeMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cluster" | "redis" => Some(Self::Cluster),
            "embedded" | "memory" => Some(Self::Embedded),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        std::env::var("CHAKRA_RUNTIME_MODE")
            .ok()
            .and_then(|v| Self::parse(&v))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// API server listen address
    pub listen_addr: String,
    /// Pool reserve refresh interval (seconds). Keep short so quotes track live
    /// reserves.
    pub refresh_interval_secs: u64,
    /// Full pool discovery interval (seconds): re-run `get_trading_pairs` and
    /// replace the graph.
    pub discovery_interval_secs: u64,
    /// Price impact threshold (bps) above which split optimization is
    /// attempted.
    pub split_threshold_bps: u32,
    /// Also try split when the second-best path is within this delta (bps) of
    /// the best path.
    pub split_competitive_delta_bps: u32,
    /// Drop split legs whose expected output is below this share of total
    /// output.
    pub min_split_fraction_bps: u32,
    /// Maximum number of candidate paths to consider for split optimization.
    pub max_splits: usize,
    /// Path finder: max hops per path (direct pools are always enumerated
    /// separately).
    pub path_finder_max_hops: usize,
    /// Path finder: cap on 2+ hop paths per quote.
    pub path_finder_max_multi_hop_paths: usize,
    /// Path finder: cap on 1-hop pools (`0` = all direct pools in graph).
    pub path_finder_max_direct_paths: usize,
    /// Allow API to RPC-fetch xy=k pool misses (default false — worker writes
    /// Redis).
    pub quote_rpc_hydrate_enabled: bool,
    /// Max xy=k pools to RPC-fetch per quote when `quote_rpc_hydrate_enabled`
    /// is true.
    pub quote_hydrate_max_pools: usize,
    /// `cluster` (Redis worker) or `embedded` (in-process worker + memory).
    pub runtime_mode: RuntimeMode,
    /// Optional snapshot backend selector (`file`, `redis`, or `memory`).
    pub snapshot_backend: Option<String>,
    /// Optional directory containing file-backed market snapshots.
    pub snapshot_dir: Option<String>,
    /// Redis URL for shared snapshot storage.
    pub snapshot_redis_url: Option<String>,
    /// Chakra EVM HTTP RPC URL (public Arc + documented failovers only).
    pub chakra_rpc_http: String,
    /// Aggregator contract address for `build_tx` `to` (empty until T5.2).
    pub chakra_aggregator: String,
    /// Allowlisted CORS origins (comma-separated).
    pub chakra_cors_origins: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:3100".to_string(),
            refresh_interval_secs: 5,
            discovery_interval_secs: 600,
            split_threshold_bps: 5,
            split_competitive_delta_bps: 50,
            min_split_fraction_bps: 5,
            max_splits: 5,
            path_finder_max_hops: 3,
            path_finder_max_multi_hop_paths: 50,
            path_finder_max_direct_paths: 0,
            quote_rpc_hydrate_enabled: false,
            quote_hydrate_max_pools: 12,
            runtime_mode: RuntimeMode::Cluster,
            snapshot_backend: None,
            snapshot_dir: None,
            snapshot_redis_url: None,
            chakra_rpc_http: dex_adapters::evm_rpc::ARC_RPC_HTTP.to_string(),
            chakra_aggregator: String::new(),
            chakra_cors_origins: vec!["http://localhost:3000".to_string()],
        }
    }
}

impl AppConfig {
    /// Load config from environment variables, falling back to defaults.
    pub fn from_env() -> Self {
        Self {
            listen_addr: std::env::var("CHAKRA_LISTEN_ADDR")
                .or_else(|_| std::env::var("LISTEN_ADDR"))
                .unwrap_or_else(|_| Self::default().listen_addr),
            refresh_interval_secs: std::env::var("REFRESH_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            discovery_interval_secs: std::env::var("DISCOVERY_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(600),
            split_threshold_bps: std::env::var("SPLIT_THRESHOLD_BPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().split_threshold_bps),
            split_competitive_delta_bps: std::env::var("SPLIT_COMPETITIVE_DELTA_BPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().split_competitive_delta_bps),
            min_split_fraction_bps: std::env::var("MIN_SPLIT_FRACTION_BPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().min_split_fraction_bps),
            max_splits: std::env::var("MAX_SPLITS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().max_splits),
            path_finder_max_hops: std::env::var("PATH_FINDER_MAX_HOPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().path_finder_max_hops),
            path_finder_max_multi_hop_paths: std::env::var("PATH_FINDER_MAX_MULTI_HOP_PATHS")
                .or_else(|_| std::env::var("PATH_FINDER_MAX_PATHS"))
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().path_finder_max_multi_hop_paths),
            path_finder_max_direct_paths: std::env::var("PATH_FINDER_MAX_DIRECT_PATHS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().path_finder_max_direct_paths),
            quote_rpc_hydrate_enabled: std::env::var("QUOTE_RPC_HYDRATE_ENABLED")
                .ok()
                .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(Self::default().quote_rpc_hydrate_enabled),
            quote_hydrate_max_pools: std::env::var("QUOTE_HYDRATE_MAX_POOLS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(Self::default().quote_hydrate_max_pools),
            runtime_mode: RuntimeMode::from_env(),
            snapshot_backend: std::env::var("SNAPSHOT_BACKEND").ok(),
            snapshot_dir: std::env::var("SNAPSHOT_DIR").ok(),
            snapshot_redis_url: std::env::var("CHAKRA_REDIS_URL").ok(),
            chakra_rpc_http: parse_chakra_rpc_http(std::env::var("CHAKRA_RPC_HTTP").ok())
                .unwrap_or_else(|_| Self::default().chakra_rpc_http),
            chakra_aggregator: std::env::var("CHAKRA_AGGREGATOR")
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase(),
            chakra_cors_origins: std::env::var("CHAKRA_CORS_ORIGINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::sync::{Mutex, OnceLock},
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn default_config_cluster_mode() {
        let config = AppConfig::default();
        assert_eq!(config.runtime_mode, RuntimeMode::Cluster);
    }

    #[test]
    fn runtime_mode_parses_embedded_aliases() {
        assert_eq!(RuntimeMode::parse("embedded"), Some(RuntimeMode::Embedded));
        assert_eq!(RuntimeMode::parse("memory"), Some(RuntimeMode::Embedded));
        assert_eq!(RuntimeMode::parse("cluster"), Some(RuntimeMode::Cluster));
    }

    #[test]
    fn default_max_splits_is_five() {
        let config = AppConfig::default();
        assert_eq!(
            config.max_splits, 5,
            "max_splits default should be 5 for multi-venue splits"
        );
    }

    #[test]
    fn chakra_redis_url_is_read_from_the_active_environment_key() {
        let _guard = env_lock().lock().unwrap();
        let original_chakra = std::env::var("CHAKRA_REDIS_URL").ok();
        std::env::set_var("CHAKRA_REDIS_URL", "redis://chakra:6379");

        let config = AppConfig::from_env();
        assert_eq!(config.snapshot_redis_url.as_deref(), Some("redis://chakra:6379"));

        match original_chakra {
            Some(v) => std::env::set_var("CHAKRA_REDIS_URL", v),
            None => std::env::remove_var("CHAKRA_REDIS_URL"),
        }
    }

    #[test]
    fn chakra_listen_addr_takes_precedence() {
        let _guard = env_lock().lock().unwrap();
        let original_chakra = std::env::var("CHAKRA_LISTEN_ADDR").ok();
        let original_listen = std::env::var("LISTEN_ADDR").ok();
        std::env::set_var("CHAKRA_LISTEN_ADDR", "0.0.0.0:9999");

        let config = AppConfig::from_env();
        assert_eq!(config.listen_addr, "0.0.0.0:9999");

        match original_chakra {
            Some(v) => std::env::set_var("CHAKRA_LISTEN_ADDR", v),
            None => std::env::remove_var("CHAKRA_LISTEN_ADDR"),
        }
        match original_listen {
            Some(v) => std::env::set_var("LISTEN_ADDR", v),
            None => std::env::remove_var("LISTEN_ADDR"),
        }
    }
}
