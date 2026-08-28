//! Minimal pool index stub for Chakra EVM (Stellar pool_index stripped).

use {
    market_snapshot::{ClmmPoolRefSnapshot, SourceSnapshot},
    std::collections::{HashMap, HashSet},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoolRef {
    pub source: String,
    pub pool_address: String,
}

#[derive(Debug, Clone, Default)]
pub struct KnownPoolIndex {
    by_contract: HashMap<String, PoolRef>,
}

impl KnownPoolIndex {
    pub fn rebuild(sources: &[SourceSnapshot], clmm_pool_refs: &[ClmmPoolRefSnapshot]) -> Self {
        let mut by_contract = HashMap::new();
        for source in sources {
            for pair in &source.pairs {
                by_contract.insert(
                    pair.pool_address.clone(),
                    PoolRef {
                        source: source.source.clone(),
                        pool_address: pair.pool_address.clone(),
                    },
                );
            }
        }
        for clmm in clmm_pool_refs {
            by_contract.insert(
                clmm.pool_address.clone(),
                PoolRef {
                    source: clmm.source.clone(),
                    pool_address: clmm.pool_address.clone(),
                },
            );
        }
        Self { by_contract }
    }

    pub fn lookup(&self, contract: &str) -> Option<&PoolRef> {
        self.by_contract.get(contract)
    }

    pub fn lookup_contract(&self, contract: &str) -> Option<&PoolRef> {
        self.by_contract.get(contract)
    }

    pub fn known_sources(&self) -> HashSet<&str> {
        self.by_contract.values().map(|r| r.source.as_str()).collect()
    }
}

pub fn touched_pools_from_events(_events: &[crate::evm_rpc::EvmLog], _index: &KnownPoolIndex) -> HashSet<PoolRef> {
    HashSet::new()
}
