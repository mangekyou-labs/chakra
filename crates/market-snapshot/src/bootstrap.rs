//! Bootstrap publisher: writes the `chakra:` Redis keys (snapshot, pool
//! state, factories) and publishes the snapshot-events channel **without any
//! RPC**. The worker's discovery loop calls this after building topology from
//! env-configured factories; T3.1 tests call it directly with fixture data.

use {
    crate::{
        pool_state_store::{
            FactoryRecord, PoolStateStore, RedisPoolStateStore, StablePoolStateValue, XykPoolStateValue,
        },
        store::{RedisSnapshotStore, SnapshotStore},
        ClmmPoolSnapshot, MarketSnapshot,
    },
    anyhow::Result,
};

/// Everything a bootstrap publish needs. Pool state and factories may be
/// empty; the snapshot must contain the graph (pairs + CLMM refs).
pub struct BootstrapPublish {
    pub snapshot: MarketSnapshot,
    pub xyk_pools: Vec<XykPoolStateValue>,
    pub stable_pools: Vec<StablePoolStateValue>,
    pub clmm_pools: Vec<ClmmPoolSnapshot>,
    pub factories: Vec<FactoryRecord>,
}

/// Publish snapshot + pool keys + factories to a cluster Redis, then announce
/// the version on `chakra:snapshot:events`.
pub async fn publish_bootstrap(redis_url: &str, ttl_secs: u64, publish: &BootstrapPublish) -> Result<()> {
    let snapshot_store = RedisSnapshotStore::new(redis_url);
    snapshot_store.publish_snapshot(&publish.snapshot).await?;

    let pool_store = RedisPoolStateStore::new(redis_url, ttl_secs)?;
    pool_store.set_xyk_batch(&publish.xyk_pools).await?;
    pool_store.set_stable_batch(&publish.stable_pools).await?;
    pool_store.set_clmm_batch(&publish.clmm_pools).await?;
    pool_store.set_factories(&publish.factories).await?;

    Ok(())
}

/// Publish to an in-memory store pair (embedded mode). Publishes the snapshot,
/// writes pool state (CLMM filtered to complete coverage) and factories.
pub async fn publish_bootstrap_memory(
    snapshot_store: &crate::store::MemorySnapshotStore,
    pool_store: &crate::pool_state_store::MemoryPoolStateStore,
    publish: &BootstrapPublish,
) -> Result<()> {
    snapshot_store.publish_snapshot(&publish.snapshot).await?;
    pool_store.set_xyk_batch(&publish.xyk_pools).await?;
    pool_store.set_stable_batch(&publish.stable_pools).await?;
    pool_store.set_clmm_batch(&publish.clmm_pools).await?;
    pool_store.set_factories(&publish.factories).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            pool_state_store::MemoryPoolStateStore, store::MemorySnapshotStore, ClmmCoverageSnapshot,
            ClmmPoolRefSnapshot, SourceSnapshot, TradingPairSnapshot,
        },
        std::net::TcpStream,
        std::process::{Child, Command, Stdio},
        std::time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn mock_snapshot() -> MarketSnapshot {
        MarketSnapshot::from_sources(
            "bootstrap-1",
            1_700_000_000_000,
            "arc-testnet",
            vec![
                SourceSnapshot {
                    source: "chakra-xyk".to_string(),
                    pairs: vec![
                        TradingPairSnapshot {
                            token_a: "0x3600000000000000000000000000000000000000".to_string(),
                            token_b: "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a".to_string(),
                            pool_address: "0xXYKUSDC_EURC".to_string(),
                            fee_bps: 30,
                            dex_type: "xyk".to_string(),
                            factory: "0xXYKFACTORY".to_string(),
                        },
                        TradingPairSnapshot {
                            token_a: "0x3600000000000000000000000000000000000000".to_string(),
                            token_b: "0xMBTC".to_string(),
                            pool_address: "0xXYKUSDC_MBTC".to_string(),
                            fee_bps: 30,
                            dex_type: "xyk".to_string(),
                            factory: "0xXYKFACTORY".to_string(),
                        },
                    ],
                },
                SourceSnapshot {
                    source: "chakra-stable".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: "0x3600000000000000000000000000000000000000".to_string(),
                        token_b: "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a".to_string(),
                        pool_address: "0xSTABLEUSDC_EURC".to_string(),
                        fee_bps: 4,
                        dex_type: "stable".to_string(),
                        factory: "0xSTABLEFACTORY".to_string(),
                    }],
                },
            ],
        )
    }

    fn fixture_xyk() -> XykPoolStateValue {
        XykPoolStateValue::new(
            "chakra-xyk",
            "0xXYKUSDC_EURC",
            "0x3600000000000000000000000000000000000000",
            "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a",
            30,
            10_000_000_000,
            10_000_000_000,
        )
    }

    fn fixture_stable() -> StablePoolStateValue {
        StablePoolStateValue::new(
            "chakra-stable",
            "0xSTABLEUSDC_EURC",
            "0x3600000000000000000000000000000000000000",
            "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a",
            200_000_000_000,
            200_000_000_000,
            100,
            4,
        )
    }

    fn fixture_clmm() -> ClmmPoolSnapshot {
        ClmmPoolSnapshot {
            source: "chakra-clmm".to_string(),
            pool_address: "0xCLMMUSDC_MBTC".to_string(),
            token0: "0x3600000000000000000000000000000000000000".to_string(),
            token1: "0xMBTC".to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [0; 4],
            tick: 0,
            liquidity: 1_000_000_000_000,
            factory: "0xCLMMFACTORY".to_string(),
            ticks: vec![],
            chunk_bitmaps: vec![],
            word_bitmaps: vec![],
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: None,
                max_loaded_tick: None,
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        }
    }

    fn fixture_factories() -> Vec<FactoryRecord> {
        vec![
            FactoryRecord::new("0xXYKFACTORY", "xyk", "chakra-xyk"),
            FactoryRecord::new("0xSTABLEFACTORY", "stable", "chakra-stable"),
            FactoryRecord::new("0xCLMMFACTORY", "clmm", "chakra-clmm"),
        ]
    }

    fn fixture_publish() -> BootstrapPublish {
        BootstrapPublish {
            snapshot: mock_snapshot().with_clmm_pool_refs(vec![ClmmPoolRefSnapshot::from_pool(&fixture_clmm())]),
            xyk_pools: vec![fixture_xyk()],
            stable_pools: vec![fixture_stable()],
            clmm_pools: vec![fixture_clmm()],
            factories: fixture_factories(),
        }
    }

    fn redis_server_guard(port: u16) -> Option<(Child, std::path::PathBuf)> {
        let dir = std::env::temp_dir().join(format!(
            "chakra-bootstrap-{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok()?;
        let child = Command::new("redis-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--save")
            .arg("")
            .arg("--appendonly")
            .arg("no")
            .arg("--dir")
            .arg(&dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        for _ in 0..40 {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Some((child, dir));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        None
    }

    #[tokio::test]
    async fn memory_bootstrap_publishes_snapshot_pools_and_factories() {
        let snapshots = MemorySnapshotStore::new();
        let pools = MemoryPoolStateStore::new();

        publish_bootstrap_memory(&snapshots, &pools, &fixture_publish())
            .await
            .unwrap();

        let loaded = snapshots.load_current_snapshot().await.unwrap();
        assert_eq!(loaded.version, "bootstrap-1");
        assert_eq!(loaded.sources.len(), 2);

        let xyk = pools
            .fetch_xyk(&[("chakra-xyk".into(), "0xXYKUSDC_EURC".into())])
            .await
            .unwrap();
        assert!(xyk.contains_key("chakra-xyk:0xXYKUSDC_EURC"));

        let stable = pools
            .fetch_stable(&[("chakra-stable".into(), "0xSTABLEUSDC_EURC".into())])
            .await
            .unwrap();
        assert!(stable.contains_key("chakra-stable:0xSTABLEUSDC_EURC"));

        let clmm = pools
            .fetch_clmm(&[("chakra-clmm".into(), "0xCLMMUSDC_MBTC".into())])
            .await
            .unwrap();
        assert!(clmm.contains_key("chakra-clmm:0xCLMMUSDC_MBTC"));

        assert_eq!(pools.fetch_factories().await.unwrap(), fixture_factories());
        assert!(crate::ready::memory_ready(&snapshots, &pools).await);
    }

    #[tokio::test]
    async fn memory_bootstrap_skips_incomplete_clmm() {
        let snapshots = MemorySnapshotStore::new();
        let pools = MemoryPoolStateStore::new();
        let mut publish = fixture_publish();
        publish.clmm_pools[0].coverage = Some(ClmmCoverageSnapshot {
            is_complete: false,
            min_loaded_tick: None,
            max_loaded_tick: None,
            scanned_word_start: None,
            scanned_word_end: None,
        });

        publish_bootstrap_memory(&snapshots, &pools, &publish).await.unwrap();

        let clmm = pools
            .fetch_clmm(&[("chakra-clmm".into(), "0xCLMMUSDC_MBTC".into())])
            .await
            .unwrap();
        assert!(clmm.is_empty());
        assert!(crate::ready::memory_ready(&snapshots, &pools).await);
    }

    #[tokio::test]
    async fn redis_bootstrap_writes_keys_events_and_readiness() {
        let port = 6397;
        let Some((_redis, _dir)) = redis_server_guard(port) else {
            eprintln!("redis-server unavailable; skipping Redis bootstrap test");
            return;
        };
        let redis_url = format!("redis://127.0.0.1:{port}/");

        publish_bootstrap(&redis_url, 86_400, &fixture_publish()).await.unwrap();

        let snapshot_store = RedisSnapshotStore::new(&redis_url);
        let loaded = snapshot_store.load_current_snapshot().await.unwrap();
        assert_eq!(loaded.version, "bootstrap-1");
        assert_eq!(loaded.sources.len(), 2);

        let pool_store = RedisPoolStateStore::with_default_ttl(&redis_url).unwrap();
        let xyk = pool_store
            .fetch_xyk(&[("chakra-xyk".into(), "0xXYKUSDC_EURC".into())])
            .await
            .unwrap();
        assert!(xyk.contains_key("chakra-xyk:0xXYKUSDC_EURC"));
        let stable = pool_store
            .fetch_stable(&[("chakra-stable".into(), "0xSTABLEUSDC_EURC".into())])
            .await
            .unwrap();
        assert!(stable.contains_key("chakra-stable:0xSTABLEUSDC_EURC"));
        assert_eq!(pool_store.fetch_factories().await.unwrap(), fixture_factories());

        assert!(crate::ready::cluster_ready(&redis_url).await.unwrap());
    }
}
