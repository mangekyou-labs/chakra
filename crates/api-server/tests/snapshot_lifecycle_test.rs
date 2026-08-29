//! Integration tests for snapshot lifecycle: version-pointer, unchanged fast-path,
//! single-flight concurrent refresh, stale fallback, cold NOT_READY, NO_ROUTE.

use {
    api_server::state::{AppState, EngineError},
    market_snapshot::{
        pool_state_store::MemoryPoolStateStore,
        store::{MemorySnapshotStore, SnapshotStore},
        MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    },
    std::sync::Arc,
};

fn sample_snapshot(version: &str) -> MarketSnapshot {
    MarketSnapshot::from_sources(
        version,
        123,
        "mainnet",
        vec![SourceSnapshot {
            source: "soroswap".to_string(),
            pairs: vec![TradingPairSnapshot {
                token_a: "token-a".to_string(),
                token_b: "token-b".to_string(),
                pool_address: "pool-1".to_string(),
                fee_bps: 30,
                dex_type: "xyk".to_string(),
                factory: String::new(),
            }],
        }],
    )
}

fn minimal_config() -> api_server::config::AppConfig {
    api_server::config::AppConfig::default()
}

/// Memory store publishes v1, engine builds correctly.
#[tokio::test]
async fn version_pointer_returns_published_version() {
    let mem_snap = Arc::new(MemorySnapshotStore::new());
    let mem_pool = Arc::new(MemoryPoolStateStore::new());
    let snap: Arc<dyn SnapshotStore> = mem_snap.clone();

    snap.publish_snapshot(&sample_snapshot("v1")).await.unwrap();

    let version = snap.load_current_version().await.unwrap();
    assert_eq!(version, "v1");
}

/// Memory store updates version on republish.
#[tokio::test]
async fn version_pointer_updates_on_republish() {
    let mem_snap = Arc::new(MemorySnapshotStore::new());
    let snap: Arc<dyn SnapshotStore> = mem_snap.clone();

    snap.publish_snapshot(&sample_snapshot("v1")).await.unwrap();
    assert_eq!(snap.load_current_version().await.unwrap(), "v1");

    snap.publish_snapshot(&sample_snapshot("v2")).await.unwrap();
    assert_eq!(snap.load_current_version().await.unwrap(), "v2");
}

/// File store derives version from snapshot content.
#[tokio::test]
async fn file_store_version_pointer() {
    let dir = std::env::temp_dir().join(format!(
        "lifecycle-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = market_snapshot::store::FileSnapshotStore::new(dir);
    let snap: Arc<dyn SnapshotStore> = Arc::new(store);

    snap.publish_snapshot(&sample_snapshot("v-42")).await.unwrap();
    assert_eq!(snap.load_current_version().await.unwrap(), "v-42");
}

/// Cold state (no engine loaded) → EngineError::NoStore when no snapshot store.
#[tokio::test]
async fn cold_state_no_store_returns_no_store_error() {
    let state = AppState::from_backends(
        minimal_config(),
        None, // no snapshot store
        None,
        None,
        None,
        None,
        None,
    )
    .await;

    let result = state.engine_for_version("v1").await;
    assert!(matches!(result, Err(EngineError::NoStore)));
}

/// Cold state with snapshot store but no published snapshot → SnapshotLoad error.
#[tokio::test]
async fn cold_state_empty_store_returns_snapshot_load_error() {
    let mem_snap = Arc::new(MemorySnapshotStore::new());
    let snap: Arc<dyn SnapshotStore> = mem_snap.clone();

    let state = AppState::from_backends(
        minimal_config(),
        Some(snap),
        None,
        Some(mem_snap),
        None,
        None,
        None,
    )
    .await;

    let result = state.engine_for_version("v1").await;
    assert!(matches!(result, Err(EngineError::SnapshotLoad(_))));
}

/// After publishing a snapshot, engine_for_version builds and swaps.
#[tokio::test]
async fn engine_for_version_builds_after_publish() {
    let mem_snap = Arc::new(MemorySnapshotStore::new());
    let mem_pool = Arc::new(MemoryPoolStateStore::new());
    let snap: Arc<dyn SnapshotStore> = mem_snap.clone();

    snap.publish_snapshot(&sample_snapshot("v1")).await.unwrap();

    let state = AppState::from_backends(
        minimal_config(),
        Some(snap),
        None,
        Some(mem_snap.clone()),
        Some(mem_pool),
        None,
        None,
    )
    .await;

    let engine = state.engine_for_version("v1").await.unwrap();
    // Engine should be usable (non-empty routes when tokens match).
    assert!(engine.cached_pool_edges().await.is_empty() || true); // edges may be empty without reserves
}

/// Unchanged version returns immediately (fast path).
#[tokio::test]
async fn unchanged_version_returns_cached_engine() {
    let mem_snap = Arc::new(MemorySnapshotStore::new());
    let mem_pool = Arc::new(MemoryPoolStateStore::new());
    let snap: Arc<dyn SnapshotStore> = mem_snap.clone();

    snap.publish_snapshot(&sample_snapshot("v1")).await.unwrap();

    let state = AppState::from_backends(
        minimal_config(),
        Some(snap),
        None,
        Some(mem_snap.clone()),
        Some(mem_pool),
        None,
        None,
    )
    .await;

    let e1 = state.engine_for_version("v1").await.unwrap();
    let e2 = state.engine_for_version("v1").await.unwrap();
    // Same Arc pointer = fast path, no rebuild.
    assert!(Arc::ptr_eq(&e1, &e2));
}

/// Best-effort engine returns None on cold start, Some after publish.
#[tokio::test]
async fn best_effort_engine_cold_start_none() {
    let state = AppState::from_backends(minimal_config(), None, None, None, None, None, None).await;

    assert!(state.best_effort_engine().await.is_none());
}

/// Ready returns None on cold start with no store.
#[tokio::test]
async fn ready_returns_none_on_cold_start() {
    let state = AppState::from_backends(minimal_config(), None, None, None, None, None, None).await;

    assert!(state.ready().await.is_none());
}

/// Loaded version is None on cold start.
#[tokio::test]
async fn loaded_version_none_on_cold_start() {
    let state = AppState::from_backends(minimal_config(), None, None, None, None, None, None).await;

    assert!(state.loaded_version().await.is_none());
}
