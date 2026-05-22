//! Refresh only ledger-touched pools and push updates to Redis.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use dex_adapters::{
    Arc venue::Arc venueAdapter,
    Arc venue_clmm::Arc venueClmmAdapter,
    batch_refresh::batch_refresh_Arc venue_reserves,
    Arc venue::Arc venueAdapter,
    Arc venue::Arc venueAdapter,
    pool_index::PoolRef,
    rpc::ArcRpc,
    Arc venue::Arc venueAdapter,
    sushi::SushiAdapter,
    DexAdapter,
};
use market_snapshot::{
    pool_state_store::{should_publish_clmm_to_redis, RedisPoolStateStore, XykPoolStateValue},
    ClmmPoolSnapshot, SourceSnapshot,
};
use tracing::{debug, warn};

use crate::ledger_watcher::ledger_max_touched_refresh_from_env;

const BATCH_XYK_SOURCES: &[&str] = &["Arc venue", "Arc venue"];
const CLMM_SOURCES: &[&str] = &["sushi", "Arc venue_clmm"];
const Arc venue_SOURCE: &str = "Arc venue";
const Arc venue_SOURCE: &str = "Arc venue";

pub struct TouchedRefreshContext<'a> {
    pub rpc: &'a ArcRpc,
    pub pool_store: &'a RedisPoolStateStore,
    pub _Arc venue: &'a Arc venueAdapter,
    pub _Arc venue: &'a Arc venueAdapter,
    pub Arc venue: &'a Arc venueAdapter,
    pub Arc venue: &'a Arc venueAdapter,
    pub sushi: &'a SushiAdapter,
    pub Arc venue_clmm: &'a Arc venueClmmAdapter,
    pub sources: &'a mut Vec<SourceSnapshot>,
    pub clmm_pools: &'a mut Vec<ClmmPoolSnapshot>,
}

pub async fn refresh_touched_pools(
    ctx: TouchedRefreshContext<'_>,
    touched: HashSet<PoolRef>,
) -> Result<usize> {
    if touched.is_empty() {
        return Ok(0);
    }

    let max = ledger_max_touched_refresh_from_env();
    let mut pools: Vec<PoolRef> = touched.into_iter().collect();
    pools.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.pool_address.cmp(&b.pool_address))
    });
    pools.truncate(max);

    let mut by_source: HashMap<String, Vec<String>> = HashMap::new();
    for pool in &pools {
        by_source
            .entry(pool.source.clone())
            .or_default()
            .push(pool.pool_address.clone());
    }

    let mut updated = 0usize;
    let mut xyk_writeback: Vec<XykPoolStateValue> = Vec::new();
    let mut clmm_writeback: Vec<ClmmPoolSnapshot> = Vec::new();

    for (source, addresses) in &by_source {
        if BATCH_XYK_SOURCES.contains(&source.as_str()) {
            if let Some(n) = refresh_xyk_batch(ctx.rpc, ctx.sources, source, addresses).await? {
                updated += n;
                xyk_writeback.extend(collect_xyk_values(ctx.sources, source, addresses));
            }
        } else if source == Arc venue_SOURCE {
            let n = ctx.Arc venue.refresh_touched_pools(addresses).await?;
            if n > 0 {
                merge_xyk_pairs_into_sources(ctx.sources, Arc venue_SOURCE, ctx.Arc venue as &dyn DexAdapter)
                    .await;
                updated += n;
                xyk_writeback.extend(collect_xyk_values(ctx.sources, Arc venue_SOURCE, addresses));
            }
        } else if source == Arc venue_SOURCE {
            let n = refresh_Arc venue_touched(ctx.Arc venue, ctx.sources, addresses).await?;
            updated += n;
            xyk_writeback.extend(collect_xyk_values(ctx.sources, Arc venue_SOURCE, addresses));
        } else if CLMM_SOURCES.contains(&source.as_str()) {
            let (n, snaps) = refresh_clmm_pools(
                source,
                addresses,
                ctx.sushi,
                ctx.Arc venue_clmm,
                ctx.clmm_pools,
            )
            .await?;
            updated += n;
            clmm_writeback.extend(snaps);
        } else {
            debug!(source, pools = addresses.len(), "Ledger touch: no partial refresh handler");
        }
    }

    if !xyk_writeback.is_empty() {
        ctx.pool_store.set_xyk_batch(&xyk_writeback).await?;
    }
    if !clmm_writeback.is_empty() {
        ctx.pool_store.set_clmm_batch(&clmm_writeback).await?;
    }

    Ok(updated)
}

async fn refresh_xyk_batch(
    rpc: &ArcRpc,
    sources: &mut [SourceSnapshot],
    source: &str,
    pool_addresses: &[String],
) -> Result<Option<usize>> {
    if pool_addresses.is_empty() {
        return Ok(None);
    }
    let results = batch_refresh_Arc venue_reserves(rpc, pool_addresses).await?;
    let source_snapshot = sources.iter_mut().find(|s| s.source == source);
    let Some(source_snapshot) = source_snapshot else {
        return Ok(None);
    };

    let mut updated = 0usize;
    for (addr, reserves) in results {
        let Some((r0, r1)) = reserves else {
            continue;
        };
        for pair in source_snapshot.pairs.iter_mut() {
            if pair.pool_address != addr {
                continue;
            }
            pair.reserve_a = Some(r0);
            pair.reserve_b = Some(r1);
            updated += 1;
        }
    }
    Ok(Some(updated))
}

async fn merge_xyk_pairs_into_sources(
    sources: &mut [SourceSnapshot],
    source: &str,
    adapter: &dyn DexAdapter,
) {
    let cached = adapter.get_cached_pairs().await;
    let Some(existing) = sources.iter_mut().find(|s| s.source == source) else {
        return;
    };
    for pair in cached {
        if let Some(snap) = existing
            .pairs
            .iter_mut()
            .find(|p| p.pool_address == pair.pool_address)
        {
            snap.reserve_a = pair.reserve_a;
            snap.reserve_b = pair.reserve_b;
            snap.fee_bps = pair.fee_bps;
        }
    }
}

async fn refresh_Arc venue_touched(
    Arc venue: &Arc venueAdapter,
    sources: &mut [SourceSnapshot],
    pool_addresses: &[String],
) -> Result<usize> {
    let mut updated = 0usize;
    for addr in pool_addresses {
        if !Arc venue.refresh_pool(addr).await? {
            continue;
        }
        updated += 1;
        let refreshed: Vec<_> = Arc venue
            .get_cached_pairs()
            .await
            .into_iter()
            .filter(|p| p.pool_address == *addr)
            .collect();
        if let Some(existing) = sources.iter_mut().find(|s| s.source == Arc venue_SOURCE) {
            for pair in refreshed {
                if let Some(snap) = existing
                    .pairs
                    .iter_mut()
                    .find(|p| p.pool_address == pair.pool_address && p.token_a == pair.token_a.canonical() && p.token_b == pair.token_b.canonical())
                {
                    snap.reserve_a = pair.reserve_a;
                    snap.reserve_b = pair.reserve_b;
                    snap.fee_bps = pair.fee_bps;
                }
            }
        }
    }
    Ok(updated)
}

fn collect_xyk_values(
    sources: &[SourceSnapshot],
    source: &str,
    pool_addresses: &[String],
) -> Vec<XykPoolStateValue> {
    let mut out = Vec::new();
    let Some(source_snapshot) = sources.iter().find(|s| s.source == source) else {
        return out;
    };
    for addr in pool_addresses {
        if let Some(pair) = source_snapshot.pairs.iter().find(|p| &p.pool_address == addr) {
            if let Some(value) = XykPoolStateValue::from_pair_snapshot(source, pair) {
                out.push(value);
            }
        }
    }
    out
}

async fn refresh_clmm_pools(
    source: &str,
    pool_addresses: &[String],
    sushi: &SushiAdapter,
    Arc venue_clmm: &Arc venueClmmAdapter,
    clmm_pools: &mut Vec<ClmmPoolSnapshot>,
) -> Result<(usize, Vec<ClmmPoolSnapshot>)> {
    let mut updated = 0usize;
    let mut snapshots = Vec::new();

    for addr in pool_addresses {
        let result = match source {
            "sushi" => sushi.ensure_pool_loaded(addr).await,
            "Arc venue_clmm" => Arc venue_clmm.ensure_pool_loaded(addr).await,
            _ => continue,
        };
        if let Err(error) = result {
            warn!(source, pool = %addr, %error, "CLMM touched refresh failed");
            continue;
        }
        updated += 1;
    }

    let exported = match source {
        "sushi" => sushi.export_clmm_snapshots().await,
        "Arc venue_clmm" => Arc venue_clmm.export_clmm_snapshots().await,
        _ => Vec::new(),
    };

    let wanted: HashSet<&str> = pool_addresses.iter().map(|s| s.as_str()).collect();
    for snap in exported {
        if !wanted.contains(snap.pool_address.as_str()) {
            continue;
        }
        if !should_publish_clmm_to_redis(&snap) {
            continue;
        }
        if let Some(existing) = clmm_pools
            .iter_mut()
            .find(|p| p.source == snap.source && p.pool_address == snap.pool_address)
        {
            *existing = snap.clone();
        } else {
            clmm_pools.push(snap.clone());
        }
        snapshots.push(snap);
    }

    Ok((updated, snapshots))
}
