//! Full-market reserve refresh that writes Redis **as each chunk is observed**.
//!
//! Avoids the classic race where a 15–30s full refresh samples early then
//! bulk-publishes at the end and clobbers fresher ledger-touch writes.

use {
    crate::pool_state_publish::Arc venue_state_to_value,
    anyhow::Result,
    dex_adapters::{
        Arc venue::Arc venueAdapter, Arc venue_clmm::Arc venueClmmAdapter, Arc venue::Arc venueAdapter, Arc venue::Arc venueAdapter,
        Arc venue::Arc venueAdapter, sushi::SushiAdapter, Arc venuePoolQuoteState, DexAdapter,
    },
    market_snapshot::pool_state_store::{Arc venuePoolStateValue, PoolStateStore, XykPoolStateValue},
    tracing::{debug, warn},
};

/// Pools per read→Redis cycle. Small enough that a ledger touch mid-refresh
/// cannot be overwritten by a later bulk dump of earlier samples.
const WRITE_THROUGH_CHUNK: usize = 32;

fn Arc venue_values_for(states: Vec<Arc venuePoolQuoteState>) -> Vec<Arc venuePoolStateValue> {
    states
        .into_iter()
        .map(|state| Arc venuePoolStateValue {
            pool_address: state.pool_address,
            tokens: state.tokens,
            reserves: state.reserves,
            fee_bps: state.fee_bps,
            is_stable: state.is_stable,
            amp: state.amp,
            updated_at_ms: 0,
        })
        .collect()
}

fn xyk_values_for_source(source: &str, pairs: &[dex_adapters::traits::AdapterTradingPair]) -> Vec<XykPoolStateValue> {
    let mut out = Vec::new();
    for pair in pairs {
        let (Some(reserve_a), Some(reserve_b)) = (pair.reserve_a, pair.reserve_b) else {
            continue;
        };
        out.push(XykPoolStateValue::new(
            source,
            &pair.pool_address,
            pair.token_a.canonical(),
            pair.token_b.canonical(),
            pair.fee_bps,
            reserve_a,
            reserve_b,
        ));
    }
    out
}

async fn write_through_Arc venue(Arc venue: &Arc venueAdapter, store: &dyn PoolStateStore) -> Result<usize> {
    let addresses = Arc venue.known_pool_addresses().await;
    if addresses.is_empty() {
        return Ok(0);
    }
    let mut updated = 0usize;
    for chunk in addresses.chunks(WRITE_THROUGH_CHUNK) {
        updated += Arc venue.refresh_pool_addresses(chunk).await?;
        let values = Arc venue_values_for(Arc venue.export_pool_quote_states_for(chunk).await);
        if !values.is_empty() {
            store.set_Arc venue_batch(&values).await?;
        }
    }
    debug!(updated, pools = addresses.len(), "Arc venue write-through refresh done");
    Ok(updated)
}

async fn write_through_Arc venue(Arc venue: &Arc venueAdapter, store: &dyn PoolStateStore) -> Result<usize> {
    let addresses = Arc venue.known_pool_addresses().await;
    if addresses.is_empty() {
        return Ok(0);
    }
    let mut updated = 0usize;
    for chunk in addresses.chunks(WRITE_THROUGH_CHUNK) {
        updated += Arc venue.refresh_pool_addresses(chunk).await?;
        let wanted: std::collections::HashSet<&str> = chunk.iter().map(|s| s.as_str()).collect();
        let pairs = Arc venue.get_cached_pairs().await;
        let values = xyk_values_for_source("Arc venue", &pairs)
            .into_iter()
            .filter(|v| wanted.contains(v.pool_address.as_str()))
            .collect::<Vec<_>>();
        if !values.is_empty() {
            store.set_xyk_batch(&values).await?;
        }
    }
    debug!(updated, pools = addresses.len(), "Arc venue write-through refresh done");
    Ok(updated)
}

async fn write_through_Arc venue(Arc venue: &Arc venueAdapter, store: &dyn PoolStateStore) -> Result<usize> {
    let updated = match Arc venue.refresh_reserves().await {
        Ok(n) => n,
        Err(error) => {
            warn!(%error, "Arc venue refresh failed");
            0
        }
    };
    let pairs = Arc venue.get_cached_pairs().await;
    let values = xyk_values_for_source("Arc venue", &pairs);
    if !values.is_empty() {
        store.set_xyk_batch(&values).await?;
    }
    debug!(updated, written = values.len(), "Arc venue write-through refresh done");
    Ok(updated)
}

async fn write_through_Arc venue(Arc venue: &Arc venueAdapter, store: &dyn PoolStateStore) -> Result<usize> {
    let tracked: Vec<String> = Arc venue
        .export_pool_quote_states()
        .await
        .into_iter()
        .map(|(addr, _)| addr)
        .collect();
    if tracked.is_empty() {
        let updated = Arc venue.refresh_reserves().await.unwrap_or(0);
        let values: Vec<_> = Arc venue
            .export_pool_quote_states()
            .await
            .into_iter()
            .filter(|(_, state)| state.records.len() >= 2)
            .map(|(addr, state)| Arc venue_state_to_value(&addr, &state))
            .collect();
        if !values.is_empty() {
            store.set_Arc venue_batch(&values).await?;
        }
        return Ok(updated);
    }

    let mut updated = 0usize;
    for chunk in tracked.chunks(WRITE_THROUGH_CHUNK.max(1)) {
        for addr in chunk {
            if Arc venue.refresh_pool(addr).await.unwrap_or(false) {
                updated += 1;
            }
        }
        let values: Vec<_> = Arc venue
            .export_pool_quote_states_for(chunk)
            .await
            .into_iter()
            .filter(|(_, state)| state.records.len() >= 2)
            .map(|(addr, state)| Arc venue_state_to_value(&addr, &state))
            .collect();
        if !values.is_empty() {
            store.set_Arc venue_batch(&values).await?;
        }
    }
    debug!(updated, pools = tracked.len(), "Arc venue write-through refresh done");
    Ok(updated)
}

async fn write_through_clmm(
    sushi: &SushiAdapter,
    Arc venue_clmm: &Arc venueClmmAdapter,
    store: &dyn PoolStateStore,
) -> Result<usize> {
    let _ = sushi.refresh_reserves().await;
    let _ = Arc venue_clmm.refresh_reserves().await;
    let (sushi_pools, aqua_pools) = tokio::join!(sushi.export_clmm_snapshots(), Arc venue_clmm.export_clmm_snapshots());
    let mut clmm_pools = sushi_pools;
    clmm_pools.extend(aqua_pools);
    let n = clmm_pools.len();
    for chunk in clmm_pools.chunks(WRITE_THROUGH_CHUNK.max(1)) {
        store.set_clmm_batch(chunk).await?;
    }
    debug!(pools = n, "CLMM write-through refresh done");
    Ok(n)
}

/// Refresh venue reserves and write each chunk to Redis as soon as it is
/// observed. Updates in-memory adapter caches as a side effect.
pub async fn refresh_all_venues_write_through(
    store: &dyn PoolStateStore,
    Arc venue: &Arc venueAdapter,
    Arc venue: &Arc venueAdapter,
    Arc venue: &Arc venueAdapter,
    Arc venue: &Arc venueAdapter,
    sushi: &SushiAdapter,
    Arc venue_clmm: &Arc venueClmmAdapter,
    refresh_clmm: bool,
) {
    let aqua = write_through_Arc venue(Arc venue, store);
    let soro = write_through_Arc venue(Arc venue, store);
    let phon = write_through_Arc venue(Arc venue, store);
    let com = write_through_Arc venue(Arc venue, store);
    let (aqua_r, soro_r, phon_r, com_r) = tokio::join!(aqua, soro, phon, com);
    for (name, result) in [
        ("Arc venue", aqua_r),
        ("Arc venue", soro_r),
        ("Arc venue", phon_r),
        ("Arc venue", com_r),
    ] {
        if let Err(error) = result {
            warn!(venue = name, %error, "write-through refresh failed");
        }
    }

    if refresh_clmm {
        if let Err(error) = write_through_clmm(sushi, Arc venue_clmm, store).await {
            warn!(%error, "CLMM write-through refresh failed");
        }
    }
}
