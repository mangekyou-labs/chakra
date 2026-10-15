//! Ownership of live pool membership and all derived lookup/watch state.
use {
    crate::evm_watcher::pair_key,
    anyhow::Result,
    dex_adapters::{
        evm_logs::{filter_subscribe_addresses, normalize_evm_address, touched_pools_from_evm_logs, DecodedCreated},
        evm_rpc::EvmLog,
        pool_index::{KnownPoolIndex, PoolRef},
    },
    market_snapshot::{
        pool_state_store::should_publish_clmm_to_redis, ClmmPoolRefSnapshot, ClmmPoolSnapshot, SourceSnapshot,
        TradingPairSnapshot,
    },
    std::{
        collections::HashSet,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
    },
    tokio::sync::RwLock,
    tracing::info,
};

#[derive(Default)]
pub(crate) struct LivePoolTopology {
    sources: Vec<SourceSnapshot>,
    clmm_pools: Vec<ClmmPoolSnapshot>,
    generation: u64,
    index: KnownPoolIndex,
    factories: Vec<String>,
    watch: Option<Arc<RwLock<Vec<String>>>>,
    watch_revision: Arc<AtomicU64>,
}

pub(crate) struct ClmmRefresh {
    generation: u64,
    pub(crate) pool: ClmmPoolRefSnapshot,
}

impl LivePoolTopology {
    pub(crate) fn new(sources: Vec<SourceSnapshot>, clmm_pools: Vec<ClmmPoolSnapshot>) -> Self {
        let mut topology = Self {
            sources,
            clmm_pools,
            ..Default::default()
        };
        topology.rebuild();
        topology
    }
    pub(crate) fn sources(&self) -> &[SourceSnapshot] {
        &self.sources
    }
    pub(crate) fn clmm_pools(&self) -> &[ClmmPoolSnapshot] {
        &self.clmm_pools
    }
    pub(crate) fn watch_revision(&self) -> Arc<AtomicU64> {
        self.watch_revision.clone()
    }
    pub(crate) fn initialize(&mut self, factories: Vec<String>) {
        self.factories = factories;
        self.rebuild();
    }
    fn rebuild(&mut self) {
        // Normalize keys once; log lookup uses normalized addresses too.
        let mut sources = self.sources.clone();
        for source in &mut sources {
            for pair in &mut source.pairs {
                pair.pool_address = normalize_evm_address(&pair.pool_address);
            }
        }
        let refs = self
            .clmm_pools
            .iter()
            .map(|p| {
                let mut pool = ClmmPoolRefSnapshot::from_pool(p);
                pool.pool_address = normalize_evm_address(&pool.pool_address);
                pool
            })
            .collect::<Vec<_>>();
        self.index = KnownPoolIndex::rebuild(&sources, &refs);
    }
    pub(crate) fn watch_addresses(&self) -> Vec<String> {
        let mut addresses = self.factories.clone();
        addresses.extend(
            self.sources
                .iter()
                .flat_map(|s| &s.pairs)
                .map(|p| normalize_evm_address(&p.pool_address)),
        );
        addresses.extend(self.clmm_pools.iter().map(|p| normalize_evm_address(&p.pool_address)));
        filter_subscribe_addresses(&addresses)
    }
    async fn changed(&mut self) {
        self.generation += 1;
        self.rebuild();
        if let Some(watch) = &self.watch {
            *watch.write().await = self.watch_addresses();
        }
        self.watch_revision.fetch_add(1, Ordering::Relaxed);
    }
    pub(crate) async fn attach_watch(&mut self, watch: Arc<RwLock<Vec<String>>>) {
        *watch.write().await = self.watch_addresses();
        self.watch = Some(watch);
        self.watch_revision.fetch_add(1, Ordering::Relaxed);
    }
    pub(crate) async fn replace(&mut self, sources: Vec<SourceSnapshot>, pools: Vec<ClmmPoolSnapshot>) {
        self.sources = sources;
        self.clmm_pools = pools;
        self.changed().await;
    }
    pub(crate) fn touched(&self, logs: &[EvmLog]) -> HashSet<PoolRef> {
        touched_pools_from_evm_logs(logs, &self.index)
    }
    #[cfg(test)]
    pub(crate) fn lookup(&self, address: &str) -> Option<PoolRef> {
        self.index.lookup_contract(&normalize_evm_address(address)).cloned()
    }
    pub(crate) fn begin_clmm_refresh(&self, source: &str, address: &str) -> Result<ClmmRefresh> {
        let pool = self
            .clmm_pools
            .iter()
            .find(|p| p.source == source && normalize_evm_address(&p.pool_address) == normalize_evm_address(address))
            .ok_or_else(|| anyhow::anyhow!("CLMM pool {source}/{address} not in topology"))?;
        Ok(ClmmRefresh {
            generation: self.generation,
            pool: ClmmPoolRefSnapshot::from_pool(pool),
        })
    }
    pub(crate) fn accept_clmm_refresh(
        &mut self,
        refresh: ClmmRefresh,
        snapshot: ClmmPoolSnapshot,
    ) -> Option<ClmmPoolSnapshot> {
        if refresh.generation != self.generation
            || !should_publish_clmm_to_redis(&snapshot)
            || snapshot.source != refresh.pool.source
            || normalize_evm_address(&snapshot.pool_address) != normalize_evm_address(&refresh.pool.pool_address)
        {
            return None;
        }
        let current = self.clmm_pools.iter_mut().find(|p| {
            p.source == refresh.pool.source
                && normalize_evm_address(&p.pool_address) == normalize_evm_address(&refresh.pool.pool_address)
        })?;
        *current = snapshot.clone();
        Some(snapshot)
    }
    pub(crate) async fn insert_created(
        &mut self,
        created: DecodedCreated,
        source: String,
        factory: String,
    ) -> Result<bool> {
        match created {
            DecodedCreated::Xyk { token0, token1, pool } => {
                let exists = self
                    .sources
                    .iter()
                    .filter(|s| s.source == source)
                    .flat_map(|s| &s.pairs)
                    .any(|p| normalize_evm_address(&p.pool_address) == normalize_evm_address(&pool));
                if exists {
                    return Ok(false);
                }
                let (token_a, token_b) = pair_key(&token0, &token1);
                self.sources.push(SourceSnapshot {
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
                let exists = self
                    .sources
                    .iter()
                    .filter(|s| s.source == source)
                    .flat_map(|s| &s.pairs)
                    .any(|p| normalize_evm_address(&p.pool_address) == normalize_evm_address(&pool));
                if exists {
                    return Ok(false);
                }
                let (token_a, token_b) = pair_key(&token0, &token1);
                self.sources.push(SourceSnapshot {
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
                let exists = self.clmm_pools.iter().any(|p| {
                    p.source == source && normalize_evm_address(&p.pool_address) == normalize_evm_address(&pool)
                });
                if exists {
                    return Ok(false);
                }
                let (token0, token1) = pair_key(&token0, &token1);
                self.clmm_pools.push(ClmmPoolSnapshot {
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
        self.changed().await;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pool(source: &str, address: &str) -> ClmmPoolSnapshot {
        ClmmPoolSnapshot {
            source: source.into(),
            pool_address: address.into(),
            token0: "a".into(),
            token1: "b".into(),
            fee_bps: 30,
            tick_spacing: 60,
            sqrt_price_x96: [0; 4],
            tick: 0,
            liquidity: 0,
            factory: "factory".into(),
            ticks: vec![],
            chunk_bitmaps: vec![],
            word_bitmaps: vec![],
            coverage: Some(market_snapshot::ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: None,
                max_loaded_tick: None,
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        }
    }
    #[tokio::test]
    async fn replacement_invalidates_refresh_even_if_same_pool_returns() {
        let mut topology = LivePoolTopology::default();
        topology.replace(vec![], vec![pool("chakra-clmm", "0xABC")]).await;
        let refresh = topology.begin_clmm_refresh("chakra-clmm", "0xabc").unwrap();
        topology.replace(vec![], vec![]).await;
        assert!(topology
            .accept_clmm_refresh(refresh, pool("chakra-clmm", "0xabc"))
            .is_none());
        assert!(topology.clmm_pools.is_empty());
        topology.replace(vec![], vec![pool("chakra-clmm", "0xabc")]).await;
        let refresh = topology.begin_clmm_refresh("chakra-clmm", "0xabc").unwrap();
        topology.replace(vec![], vec![pool("chakra-clmm", "0xabc")]).await;
        assert!(topology
            .accept_clmm_refresh(refresh, pool("chakra-clmm", "0xabc"))
            .is_none());
    }
    #[tokio::test]
    async fn created_pool_updates_lookup_watch_and_revision_together() {
        let mut topology = LivePoolTopology::default();
        let watch = Arc::new(RwLock::new(vec![]));
        topology.attach_watch(watch.clone()).await;
        let before = topology.watch_revision.load(Ordering::Relaxed);
        let address = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        topology
            .insert_created(
                DecodedCreated::Xyk {
                    token0: "a".into(),
                    token1: "b".into(),
                    pool: address.into(),
                },
                "chakra-xyk".into(),
                "factory".into(),
            )
            .await
            .unwrap();
        assert_eq!(topology.lookup(address).unwrap().source, "chakra-xyk");
        assert!(watch.read().await.iter().any(|p| p == address));
        assert!(topology.watch_revision.load(Ordering::Relaxed) > before);
        topology.replace(vec![], vec![]).await;
        assert!(topology.lookup(address).is_none());
        assert!(!watch.read().await.iter().any(|p| p == address));
    }
    #[tokio::test]
    async fn refresh_matches_source_and_case_normalized_address_and_keeps_incomplete_state() {
        let mut topology = LivePoolTopology::default();
        topology
            .replace(vec![], vec![pool("one", "0xABC"), pool("two", "0xabc")])
            .await;
        assert!(topology.begin_clmm_refresh("missing", "0xabc").is_err());
        let refresh = topology.begin_clmm_refresh("two", "0xABC").unwrap();
        let mut incomplete = pool("two", "0xabc");
        incomplete.tick = 99;
        incomplete.coverage = None;
        assert!(topology.accept_clmm_refresh(refresh, incomplete).is_none());
        assert_eq!(topology.clmm_pools[1].tick, 0);
        let refresh = topology.begin_clmm_refresh("two", "0xABC").unwrap();
        let mut complete = pool("two", "0xabc");
        complete.tick = 42;
        assert!(topology.accept_clmm_refresh(refresh, complete).is_some());
        assert_eq!(topology.clmm_pools[0].tick, 0);
        assert_eq!(topology.clmm_pools[1].tick, 42);
    }
}
