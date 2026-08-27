//! Map on-chain contract IDs to Chakra `(source, pool_address)` for ledger
//! event ingestion.
//!
//! Accepts both Arc `C…` contract ids and EVM `0x…` addresses so the same
//! index serves the Arc `eth_getLogs` path (T3.3) without dropping every pool.

use {
    crate::{
        evm_logs::{is_evm_address, normalize_evm_address},
        router_events::pools_from_router_event,
        rpc::events::ContractEvent,
        utils::is_contract_address,
    },
    market_snapshot::{ClmmPoolRefSnapshot, SourceSnapshot},
    std::collections::{HashMap, HashSet},
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
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
                let Some(address) = index_key(&pair.pool_address) else {
                    continue;
                };
                by_contract.insert(
                    address.clone(),
                    PoolRef {
                        source: source.source.clone(),
                        pool_address: address,
                    },
                );
            }
        }
        for pool in clmm_pool_refs {
            let Some(address) = index_key(&pool.pool_address) else {
                continue;
            };
            by_contract.insert(
                address.clone(),
                PoolRef {
                    source: pool.source.clone(),
                    pool_address: address,
                },
            );
        }
        Self { by_contract }
    }

    pub fn len(&self) -> usize {
        self.by_contract.len()
    }

    pub fn lookup_contract(&self, contract_id: &str) -> Option<&PoolRef> {
        self.by_contract.get(contract_id)
    }

    fn insert_if_known(&self, touched: &mut HashSet<PoolRef>, pool_address: &str) {
        if let Some(pool) = self.lookup_contract(pool_address) {
            touched.insert(pool.clone());
        }
    }
}

/// Canonical index key: Arc ids pass through, EVM addresses are lowercased.
fn index_key(address: &str) -> Option<String> {
    if is_contract_address(address) {
        Some(address.to_string())
    } else if is_evm_address(address) {
        Some(normalize_evm_address(address))
    } else {
        None
    }
}

/// Pools touched in the ledger range: direct pool contract events plus router
/// events where the pool id is carried in the event body (Arc venue
/// deposit/swap/ withdraw, Arc venue add/remove liquidity).
pub fn touched_pools_from_events(events: &[ContractEvent], index: &KnownPoolIndex) -> HashSet<PoolRef> {
    let mut touched = HashSet::new();
    for event in events {
        if event.event_type != "contract" {
            continue;
        }
        if let Some(pool) = index.lookup_contract(&event.contract_id) {
            touched.insert(pool.clone());
            continue;
        }
        for pool_address in pools_from_router_event(
            &event.contract_id,
            event.topic.as_deref(),
            crate::rpc::events::event_value_xdr(event.value.as_ref()),
        ) {
            index.insert_if_known(&mut touched, &pool_address);
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use {super::*, market_snapshot::TradingPairSnapshot};

    const POOL: &str = "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2";

    fn sample_index() -> KnownPoolIndex {
        KnownPoolIndex::rebuild(
            &[SourceSnapshot {
                source: "Arc venue".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: POOL.to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                }],
            }],
            &[],
        )
    }

    #[test]
    fn maps_events_to_known_pools() {
        let index = sample_index();
        let events = vec![ContractEvent {
            event_type: "contract".to_string(),
            ledger: 100,
            contract_id: POOL.to_string(),
            id: "1-1".to_string(),
            tx_hash: "a".repeat(64),
            ledger_closed_at: None,
            in_successful_contract_call: None,
            value: None,
            topic: None,
        }];
        let touched = touched_pools_from_events(&events, &index);
        assert_eq!(touched.len(), 1);
        let pool = touched.iter().next().unwrap();
        assert_eq!(pool.source, "Arc venue");
    }

    #[test]
    fn indexes_evm_0x_pool_addresses() {
        let evm_pool = "0x00000000000000000000000000000000000000EB";
        let index = KnownPoolIndex::rebuild(
            &[SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "0x3600000000000000000000000000000000000000".to_string(),
                    token_b: "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a".to_string(),
                    pool_address: evm_pool.to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: "0xCAFE".to_string(),
                }],
            }],
            &[],
        );
        assert_eq!(index.len(), 1);
        // Lookup is case-insensitive (log addresses are lowercase).
        let pool = index
            .lookup_contract("0x00000000000000000000000000000000000000eb")
            .unwrap();
        assert_eq!(pool.pool_address, evm_pool.to_ascii_lowercase());
        // Arc ids still indexed (dual-mode rebuild).
        let mixed = sample_index();
        assert_eq!(mixed.len(), 1);
    }

    #[test]
    fn maps_Arc venue_router_event_to_pool() {
        use {
            crate::Arc venue::Arc venue_ROUTER,
            base64::Engine,
            Arc_strkey::Contract,
            Arc_xdr::curr::{Limits, ScAddress, ScVal, WriteXdr},
        };

        let hash = [42u8; 32];
        let pool_id = format!("{}", Contract(hash));
        let index = KnownPoolIndex::rebuild(
            &[SourceSnapshot {
                source: "Arc venue".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: "A".to_string(),
                    token_b: "B".to_string(),
                    pool_address: pool_id.clone(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: String::new(),
                }],
            }],
            &[],
        );

        let body = ScVal::Vec(Some(Arc_xdr::curr::ScVec(
            vec![
                ScVal::Address(ScAddress::Contract(Arc_xdr::curr::ContractId(
                    Arc_xdr::curr::Hash(hash),
                ))),
                ScVal::U32(1),
            ]
            .try_into()
            .unwrap(),
        )));
        let topic = ScVal::Symbol(Arc_xdr::curr::ScSymbol::try_from("deposit").unwrap());
        let b64 = |v: &ScVal| base64::engine::general_purpose::STANDARD.encode(v.to_xdr(Limits::none()).unwrap());

        let events = vec![ContractEvent {
            event_type: "contract".to_string(),
            ledger: 100,
            contract_id: Arc venue_ROUTER.to_string(),
            id: "1-2".to_string(),
            tx_hash: "b".repeat(64),
            ledger_closed_at: None,
            in_successful_contract_call: None,
            value: Some(serde_json::Value::String(b64(&body))),
            topic: Some(vec![b64(&topic)]),
        }];
        let touched = touched_pools_from_events(&events, &index);
        assert_eq!(touched.len(), 1);
        assert_eq!(touched.iter().next().unwrap().pool_address, pool_id);
    }
}
