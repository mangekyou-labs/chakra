//! `/ready` predicate shared by API and worker.
//!
//! Ready = current snapshot published **and** at least one pool-state record
//! present. Cluster mode checks the real Redis keys; embedded mode checks the
//! in-memory stores.

use {
    crate::{
        pool_state_store::MemoryPoolStateStore,
        store::{MemorySnapshotStore, SNAPSHOT_CURRENT_KEY},
    },
    anyhow::Result,
};

/// Cluster (Redis) readiness: true iff `chakra:snapshot:current` exists and at
/// least one `chakra:pool:*` key is present.
pub async fn cluster_ready(redis_url: &str) -> Result<bool> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    let snapshot: bool = redis::cmd("EXISTS")
        .arg(SNAPSHOT_CURRENT_KEY)
        .query_async(&mut conn)
        .await?;
    if !snapshot {
        return Ok(false);
    }
    // SCAN the `chakra:pool:*` key space (COUNTKEYS does not exist in Redis).
    let mut pool_count: i64 = 0;
    let mut cursor: i64 = 0;
    loop {
        let (next, keys): (i64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg("chakra:pool:*")
            .arg("COUNT")
            .arg(256)
            .query_async(&mut conn)
            .await?;
        pool_count += keys.len() as i64;
        cursor = next;
        if cursor == 0 {
            break;
        }
    }
    Ok(pool_count > 0)
}

/// Embedded (memory) readiness: true iff a snapshot was published and the pool
/// store holds at least one record.
pub async fn memory_ready(snapshot_store: &MemorySnapshotStore, pool_store: &MemoryPoolStateStore) -> bool {
    snapshot_store.has_snapshot().await && pool_store.pool_count().await > 0
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            pool_state_store::{PoolStateStore, StablePoolStateValue, XykPoolStateValue},
            store::{RedisSnapshotStore, SnapshotStore},
            MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
        },
        redis::AsyncCommands,
        std::net::TcpStream,
        std::process::{Child, Command, Stdio},
        std::time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn snapshot_with_pairs() -> MarketSnapshot {
        MarketSnapshot::from_sources(
            "v1",
            123,
            "arc-testnet",
            vec![SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "USDC".to_string(),
                    token_b: "EURC".to_string(),
                    pool_address: "xyk-usdc-eurc".to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: "factory-xyk".to_string(),
                }],
            }],
        )
    }

    fn redis_server_guard(port: u16) -> Option<(Child, std::path::PathBuf)> {
        let dir = std::env::temp_dir().join(format!(
            "chakra-ready-{}",
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
    async fn cluster_ready_is_false_with_only_snapshot() {
        let port = 6399;
        let Some((_redis, _dir)) = redis_server_guard(port) else {
            eprintln!("redis-server unavailable; skipping Redis ready test");
            return;
        };
        let redis_url = format!("redis://127.0.0.1:{port}/");

        let snapshot = snapshot_with_pairs();
        let store = RedisSnapshotStore::with_options(&redis_url, "chakra:snapshot:events", 3);
        store.publish_snapshot(&snapshot).await.unwrap();

        assert!(!cluster_ready(&redis_url).await.unwrap());
    }

    #[tokio::test]
    async fn cluster_ready_is_true_with_snapshot_and_pool() {
        let port = 6398;
        let Some((_redis, _dir)) = redis_server_guard(port) else {
            eprintln!("redis-server unavailable; skipping Redis ready test");
            return;
        };
        let redis_url = format!("redis://127.0.0.1:{port}/");

        let snapshot = snapshot_with_pairs();
        let store = RedisSnapshotStore::with_options(&redis_url, "chakra:snapshot:events", 3);
        store.publish_snapshot(&snapshot).await.unwrap();

        let client = redis::Client::open(redis_url.as_str()).unwrap();
        let mut conn = client.get_multiplexed_async_connection().await.unwrap();
        conn.set_ex::<_, _, ()>("chakra:pool:xyk:chakra-xyk:xyk-usdc-eurc", b"{}", 86_400)
            .await
            .unwrap();

        assert!(cluster_ready(&redis_url).await.unwrap());
    }

    #[tokio::test]
    async fn memory_ready_requires_snapshot_and_pool() {
        let snapshots = MemorySnapshotStore::new();
        let pools = MemoryPoolStateStore::new();

        assert!(!memory_ready(&snapshots, &pools).await);

        snapshots.publish_snapshot(&snapshot_with_pairs()).await.unwrap();
        assert!(!memory_ready(&snapshots, &pools).await);

        pools
            .set_xyk_batch(&[XykPoolStateValue::new(
                "chakra-xyk",
                "xyk-usdc-eurc",
                "USDC",
                "EURC",
                30,
                10_000_000_000,
                10_000_000_000,
            )])
            .await
            .unwrap();
        assert!(memory_ready(&snapshots, &pools).await);
    }

    #[tokio::test]
    async fn memory_ready_counts_stable_pools() {
        let snapshots = MemorySnapshotStore::new();
        let pools = MemoryPoolStateStore::new();
        snapshots.publish_snapshot(&snapshot_with_pairs()).await.unwrap();

        pools
            .set_stable_batch(&[StablePoolStateValue::new(
                "chakra-stable",
                "stable-usdc-eurc",
                "USDC",
                "EURC",
                200_000_000_000,
                200_000_000_000,
                100,
                4,
            )])
            .await
            .unwrap();

        assert!(memory_ready(&snapshots, &pools).await);
    }
}
