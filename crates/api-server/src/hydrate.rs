//! Quote-time pool-state hydration (Chakra).
//!
//! `/quote` never touches RPC: it reads per-pool state from Redis
//! (`chakra:pool:*`) or the in-memory store. RPC hydrate stays disabled
//! (T4.3: `QUOTE_RPC_HYDRATE_ENABLED=false` → zero RPC calls on `/quote`).

use {
    crate::state::AppState,
    market_snapshot::pool_state_store::PoolStateStore,
    router_engine::{Path, QuoteEngine, QuoteHydration, RouteRequest},
    std::collections::HashSet,
};

fn collect_pool_refs(paths: &[Path]) -> (Vec<(String, String)>, Vec<(String, String)>, Vec<(String, String)>) {
    let mut xyk = HashSet::new();
    let mut stable = HashSet::new();
    let mut clmm = HashSet::new();
    for path in paths {
        for (source, pool_address) in path.sources.iter().zip(path.pool_addresses.iter()) {
            match source.as_str() {
                "chakra-xyk" => {
                    xyk.insert((source.clone(), pool_address.clone()));
                }
                // T-XYLO: the Xylo pool state lives in the stable bucket
                // (StablePoolStateValue with A=200).
                "chakra-stable" | "xylo" => {
                    stable.insert((source.clone(), pool_address.clone()));
                }
                "chakra-clmm" => {
                    clmm.insert((source.clone(), pool_address.clone()));
                }
                _ => {}
            }
        }
    }
    (
        xyk.into_iter().collect(),
        stable.into_iter().collect(),
        clmm.into_iter().collect(),
    )
}

/// Load per-pool state for the candidate paths (Redis or memory). Never RPC.
pub async fn hydrate_for_quote(state: &AppState, engine: &QuoteEngine, request: &RouteRequest) -> QuoteHydration {
    let paths = engine.find_candidate_paths(request).await;
    let (xyk_refs, stable_refs, clmm_refs) = collect_pool_refs(&paths);
    let store: Option<&dyn PoolStateStore> = state
        .pool_store
        .as_ref()
        .map(|s| s.as_ref())
        .or_else(|| state.memory_pool.as_ref().map(|s| s.as_ref() as &dyn PoolStateStore));
    let Some(store) = store else {
        return QuoteHydration::default();
    };
    let xyk_pools = store.fetch_xyk(&xyk_refs).await.unwrap_or_default();
    let stable_pools = store.fetch_stable(&stable_refs).await.unwrap_or_default();
    let clmm_snapshots = store.fetch_clmm(&clmm_refs).await.unwrap_or_default();
    let clmm_pools = clmm_snapshots
        .into_iter()
        .map(|(key, pool)| {
            let (state, ticks) = dex_adapters::clmm_math::clmm_pool_from_snapshot(&pool);
            (
                key,
                router_engine::SnapshotClmmQuoteState {
                    source: pool.source,
                    pool_address: pool.pool_address,
                    is_complete: pool.coverage.as_ref().map(|c| c.is_complete).unwrap_or(false),
                    pool: state,
                    ticks,
                    coverage: pool.coverage,
                },
            )
        })
        .collect();
    // T4.5: Fetch allowlisted factories from the pool store.
    let factories = store.fetch_factories().await.unwrap_or_default();
    QuoteHydration {
        xyk_pools,
        clmm_pools,
        stable_pools,
        factories,
        ..Default::default()
    }
}
