//! Public, versioned analytics response. The worker writes the additive
//! `chakra:analytics:*` namespace; when it is unavailable this module returns
//! a valid zero-history response so the dashboard can render a clear state.

use {
    chrono::{DateTime, Utc},
    market_data_worker::analytics::SwapSummary,
    router_engine::types::TradingPair,
    serde::Serialize,
    std::collections::{BTreeMap, HashMap, HashSet},
};

#[derive(Debug, Clone, Serialize)]
pub struct StatsMeta {
    pub chain: String,
    pub aggregator: String,
    pub deployment_block: u64,
    /// Latest observed Arc block at the last successful analytics poll.
    pub chain_head: u64,
    /// Confirmation-adjusted target (`chain_head - confirmations`).
    pub confirmed_head: u64,
    /// Last completely indexed block (the committed poller cursor).
    pub indexed_head: u64,
    /// `confirmed_head - indexed_head` (how far indexing trails the target).
    pub lag_blocks: u64,
    /// Age in seconds of the last *successful analytics poll*, not the age of
    /// the newest swap.
    pub freshness_secs: Option<u64>,
    pub range: String,
    pub attributed_swaps: u64,
    pub unattributed_swaps: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsOverview {
    pub stablecoin_notional_micros: String,
    pub confirmed_swaps: u64,
    pub unique_traders: u64,
    pub split_swaps: u64,
    pub split_share_bps: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyStats {
    pub day: String,
    pub stablecoin_notional_micros: String,
    pub swaps: u64,
    pub traders: u64,
    pub split_swaps: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VenueStats {
    pub source: String,
    pub label: String,
    pub swap_participation: u64,
    pub subroutes: u64,
    pub hops: u64,
    pub route_share_bps: u32,
    pub pair_usage: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteHealth {
    pub token_in: String,
    pub token_out: String,
    pub direct: bool,
    pub multihop: bool,
    pub usable_pools: u64,
    pub best_sources: Vec<String>,
    pub state_age_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsResponse {
    pub meta: StatsMeta,
    pub overview: StatsOverview,
    pub daily: Vec<DailyStats>,
    pub venues: Vec<VenueStats>,
    pub route_health: Vec<RouteHealth>,
}

/// Fold the poller's head state into `meta` using the release contract:
/// `chain_head` = latest observed Arc block, `confirmed_head` =
/// confirmation-adjusted target, `indexed_head` = last fully indexed block,
/// `lag_blocks` = `confirmed - indexed`, and `freshness_secs` = age of the
/// last *successful analytics poll* (never the age of the newest swap — a
/// quiet chain is still fresh while the poller runs).
pub fn apply_heads(meta: &mut StatsMeta, heads: Option<(u64, u64, u64)>, polled_at: Option<u64>, now_secs: u64) {
    if let Some((chain, confirmed, indexed)) = heads {
        meta.chain_head = chain;
        meta.confirmed_head = confirmed;
        meta.indexed_head = indexed;
        meta.lag_blocks = confirmed.saturating_sub(indexed);
    }
    meta.freshness_secs = polled_at.map(|polled| now_secs.saturating_sub(polled));
}

/// Route diagnostics use the same six catalog directions as readiness. The
/// engine graph is consulted without RPC; quote-time state remains Redis-only.
pub async fn route_health_for_engine(engine: &router_engine::QuoteEngine) -> Vec<RouteHealth> {
    // Snapshot topology addresses are normalized to lowercase. Keep the
    // diagnostics requests in the same canonical form; `TokenId` itself is
    // intentionally a lossless wrapper and does not normalize addresses.
    let usdc = market_snapshot::decimals::USDC_ERC20.to_ascii_lowercase();
    let eurc = market_snapshot::decimals::EURC.to_ascii_lowercase();
    let cirbtc = market_snapshot::decimals::CIRBTC.to_ascii_lowercase();
    let pairs = [(&usdc, &eurc), (&usdc, &cirbtc), (&eurc, &cirbtc)];
    let mut routes = Vec::with_capacity(6);
    for (token_in, token_out) in pairs.iter().flat_map(|(a, b)| [(*a, *b), (*b, *a)]) {
        let request = router_engine::RouteRequest {
            token_in: router_engine::TokenId::from_str_auto(token_in),
            token_out: router_engine::TokenId::from_str_auto(token_out),
            amount_in: if token_in.eq_ignore_ascii_case(market_snapshot::decimals::CIRBTC) {
                100
            } else {
                1_000_000
            },
            slippage_bps: Some(50),
            max_hops: Some(4),
            max_splits: Some(1),
        };
        let paths = engine.find_candidate_paths(&request).await;
        let direct = paths.iter().any(|path| path.hops == 1);
        let multihop = paths.iter().any(|path| path.hops > 1);
        let usable_pools = paths
            .iter()
            .flat_map(|path| path.pool_addresses.iter())
            .collect::<std::collections::HashSet<_>>()
            .len() as u64;
        let best_sources = paths.first().map(|path| path.sources.clone()).unwrap_or_default();
        routes.push(RouteHealth {
            token_in: token_in.clone(),
            token_out: token_out.clone(),
            direct,
            multihop,
            usable_pools,
            best_sources,
            state_age_secs: None,
        });
    }
    routes
}

pub async fn all_routes_healthy(engine: &router_engine::QuoteEngine) -> bool {
    route_health_for_engine(engine)
        .await
        .into_iter()
        .all(|route| route.direct || route.multihop)
}

pub fn empty(range: String, aggregator: String) -> StatsResponse {
    let usdc = market_snapshot::decimals::USDC_ERC20.to_string();
    let eurc = market_snapshot::decimals::EURC.to_string();
    let cirbtc = market_snapshot::decimals::CIRBTC.to_string();
    let mut route_health = Vec::new();
    for (a, b) in [(&usdc, &eurc), (&usdc, &cirbtc), (&eurc, &cirbtc)] {
        route_health.push(RouteHealth {
            token_in: a.clone(),
            token_out: b.clone(),
            direct: false,
            multihop: false,
            usable_pools: 0,
            best_sources: vec![],
            state_age_secs: None,
        });
        route_health.push(RouteHealth {
            token_in: b.clone(),
            token_out: a.clone(),
            direct: false,
            multihop: false,
            usable_pools: 0,
            best_sources: vec![],
            state_age_secs: None,
        });
    }
    StatsResponse {
        meta: StatsMeta {
            chain: "arc-testnet".into(),
            aggregator,
            deployment_block: 59_424_918,
            chain_head: 0,
            indexed_head: 0,
            confirmed_head: 0,
            lag_blocks: 0,
            freshness_secs: None,
            range,
            attributed_swaps: 0,
            unattributed_swaps: 0,
        },
        overview: StatsOverview {
            stablecoin_notional_micros: "0".into(),
            confirmed_swaps: 0,
            unique_traders: 0,
            split_swaps: 0,
            split_share_bps: 0,
        },
        daily: vec![],
        venues: vec![],
        route_health,
    }
}

/// Fold confirmed worker records into the public response. Amounts remain
/// integers throughout; only the final decimal-string fields are serialized.
pub fn apply_summaries(response: &mut StatsResponse, summaries: &[SwapSummary]) {
    apply_summaries_with_edges(response, summaries, &[]);
}

/// Fold records and the current topology into the venue view. Calldata has
/// pool addresses but no per-leg execution amounts, so participation and
/// route share are count-based; unknown pool addresses remain explicit.
pub fn apply_summaries_with_edges(response: &mut StatsResponse, summaries: &[SwapSummary], edges: &[TradingPair]) {
    let mut notional = 0u128;
    let mut traders = HashSet::new();
    let mut daily: BTreeMap<String, (u128, u64, HashSet<String>, u64)> = BTreeMap::new();
    let mut attributed = 0u64;
    let mut unattributed = 0u64;
    let mut split = 0u64;
    let mut pool_sources = HashMap::<String, String>::new();
    for edge in edges {
        pool_sources
            .entry(edge.pool_address.to_ascii_lowercase())
            .or_insert_with(|| edge.source.clone());
    }
    let mut venue_counts = BTreeMap::<String, (u64, u64, u64, HashSet<String>)>::new();
    let mut total_subroutes = 0u64;
    let mut has_unknown_pool = false;
    for summary in summaries {
        let value = summary.notional_micros.parse::<u128>().unwrap_or(0);
        notional = notional.saturating_add(value);
        traders.insert(summary.trader.to_ascii_lowercase());
        if summary.attributed {
            attributed += 1;
        } else {
            unattributed += 1;
        }
        if summary.split {
            split += 1;
        }
        if summary.attributed && !summary.pools.is_empty() {
            total_subroutes = total_subroutes.saturating_add(summary.subroutes as u64);
            let mut seen_sources = HashSet::new();
            for pool in &summary.pools {
                if let Some(source) = pool_sources.get(&pool.to_ascii_lowercase()) {
                    seen_sources.insert(source.clone());
                } else {
                    has_unknown_pool = true;
                }
            }
            for source in seen_sources {
                let entry = venue_counts.entry(source).or_insert_with(|| (0, 0, 0, HashSet::new()));
                entry.0 = entry.0.saturating_add(1);
                entry.1 = entry.1.saturating_add(summary.subroutes as u64);
                entry.2 = entry.2.saturating_add(summary.hops as u64);
                entry.3.insert(format!("{}→{}", summary.token_in, summary.token_out));
            }
        } else if summary.attributed && summary.pools.is_empty() {
            has_unknown_pool = true;
        }
        let day = DateTime::<Utc>::from_timestamp(summary.timestamp as i64, 0)
            .map(|date| date.date_naive().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let entry = daily.entry(day).or_insert_with(|| (0, 0, HashSet::new(), 0));
        entry.0 = entry.0.saturating_add(value);
        entry.1 += 1;
        entry.2.insert(summary.trader.to_ascii_lowercase());
        if summary.split {
            entry.3 += 1;
        }
    }
    response.meta.attributed_swaps = attributed;
    response.meta.unattributed_swaps = unattributed;
    response.overview.stablecoin_notional_micros = notional.to_string();
    response.overview.confirmed_swaps = summaries.len() as u64;
    response.overview.unique_traders = traders.len() as u64;
    response.overview.split_swaps = split;
    response.overview.split_share_bps = if summaries.is_empty() {
        0
    } else {
        ((split * 10_000) / summaries.len() as u64) as u32
    };
    response.daily = daily
        .into_iter()
        .filter(|(day, _)| day != "unknown")
        .map(|(day, (value, swaps, traders, split_swaps))| DailyStats {
            day,
            stablecoin_notional_micros: value.to_string(),
            swaps,
            traders: traders.len() as u64,
            split_swaps,
        })
        .collect();

    response.venues = venue_counts
        .into_iter()
        .map(|(source, (participation, subroutes, hops, pairs))| VenueStats {
            label: source.clone(),
            source,
            swap_participation: participation,
            subroutes,
            hops,
            route_share_bps: subroutes
                .saturating_mul(10_000)
                .checked_div(total_subroutes)
                .unwrap_or(0) as u32,
            pair_usage: pairs.into_iter().collect(),
        })
        .collect();
    if has_unknown_pool {
        response.venues.push(VenueStats {
            source: "unattributed".into(),
            label: "Unattributed".into(),
            swap_participation: 0,
            subroutes: 0,
            hops: 0,
            route_share_bps: 0,
            pair_usage: vec![],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn route_health_matches_lowercase_snapshot_addresses() {
        let engine = router_engine::QuoteEngine::new(
            router_engine::path_finder::PathFinderConfig::default(),
            router_engine::split_optimizer::SplitConfig::default(),
        );
        let usdc = market_snapshot::decimals::USDC_ERC20.to_ascii_lowercase();
        let eurc = market_snapshot::decimals::EURC.to_ascii_lowercase();
        let cirbtc = market_snapshot::decimals::CIRBTC.to_ascii_lowercase();
        engine
            .update_pairs_from_cache(
                "fixture",
                &[
                    router_engine::TradingPair {
                        token_a: router_engine::TokenId::from_str_auto(&usdc),
                        token_b: router_engine::TokenId::from_str_auto(&eurc),
                        source: "fixture".into(),
                        pool_address: "0xpool-usdc-eurc".into(),
                        fee_bps: 30,
                        reserve_a: None,
                        reserve_b: None,
                        factory: String::new(),
                        dex_type: "xyk".into(),
                    },
                    router_engine::TradingPair {
                        token_a: router_engine::TokenId::from_str_auto(&usdc),
                        token_b: router_engine::TokenId::from_str_auto(&cirbtc),
                        source: "fixture".into(),
                        pool_address: "0xpool-usdc-cirbtc".into(),
                        fee_bps: 30,
                        reserve_a: None,
                        reserve_b: None,
                        factory: String::new(),
                        dex_type: "xyk".into(),
                    },
                    router_engine::TradingPair {
                        token_a: router_engine::TokenId::from_str_auto(&eurc),
                        token_b: router_engine::TokenId::from_str_auto(&cirbtc),
                        source: "fixture".into(),
                        pool_address: "0xpool-eurc-cirbtc".into(),
                        fee_bps: 30,
                        reserve_a: None,
                        reserve_b: None,
                        factory: String::new(),
                        dex_type: "xyk".into(),
                    },
                ],
            )
            .await;

        let routes = route_health_for_engine(&engine).await;
        assert_eq!(routes.len(), 6);
        assert!(routes.iter().all(|route| route.direct));
    }

    #[test]
    fn zero_history_contract_has_all_directed_catalog_pairs() {
        let response = empty("30d".to_string(), "0xaggregator".to_string());
        assert_eq!(response.meta.range, "30d");
        assert_eq!(response.overview.stablecoin_notional_micros, "0");
        assert_eq!(response.route_health.len(), 6);
        assert!(response
            .route_health
            .iter()
            .all(|route| !route.direct && !route.multihop));
        assert_eq!(response.meta.deployment_block, 59_424_918);
    }

    #[test]
    fn head_meta_follows_the_chain_confirmed_indexed_lag_contract() {
        let mut response = empty("30d".to_string(), "0xaggregator".to_string());
        // Worker just caught up: chain 200, confirmed target 195, cursor 195.
        apply_heads(
            &mut response.meta,
            Some((200, 195, 195)),
            Some(1_700_000_000),
            1_700_000_030,
        );
        assert_eq!(response.meta.chain_head, 200, "chain_head is the latest observed block");
        assert_eq!(response.meta.confirmed_head, 195, "confirmed_head is the confirmation-adjusted target");
        assert_eq!(response.meta.indexed_head, 195, "indexed_head is the committed cursor");
        assert_eq!(response.meta.lag_blocks, 0);
        // Freshness tracks the poll, not the newest swap.
        assert_eq!(response.meta.freshness_secs, Some(30));
    }

    #[test]
    fn head_meta_reports_lag_and_missing_freshness_honestly() {
        let mut response = empty("30d".to_string(), "0xaggregator".to_string());
        // Chain 100, confirmed 90, but indexing only reached 60.
        apply_heads(&mut response.meta, Some((100, 90, 60)), None, 1_700_000_000);
        assert_eq!(response.meta.chain_head, 100);
        assert_eq!(response.meta.confirmed_head, 90);
        assert_eq!(response.meta.indexed_head, 60);
        assert_eq!(response.meta.lag_blocks, 30, "lag is confirmed_head - indexed_head");
        assert_eq!(
            response.meta.freshness_secs, None,
            "no successful poll yet -> freshness must stay None, never swap age"
        );
    }

    #[test]
    fn venue_stats_map_known_pools_and_keep_unknown_explicit() {
        let mut response = empty("30d".to_string(), "0xaggregator".to_string());
        let summary = SwapSummary {
            tx_hash: "0xtx".into(),
            trader: "0xtrader".into(),
            block: 1,
            timestamp: 1_700_000_000,
            token_in: market_snapshot::decimals::USDC_ERC20.into(),
            token_out: market_snapshot::decimals::CIRBTC.into(),
            amount_in: "100".into(),
            amount_out: "90".into(),
            split: true,
            attributed: true,
            notional_micros: "100".into(),
            subroutes: 2,
            hops: 3,
            pools: vec!["0xpool".into(), "0xunknown".into()],
        };
        let edge = TradingPair {
            token_a: router_engine::TokenId::from_str_auto(market_snapshot::decimals::USDC_ERC20),
            token_b: router_engine::TokenId::from_str_auto(market_snapshot::decimals::CIRBTC),
            source: "unitflow".into(),
            pool_address: "0xpool".into(),
            fee_bps: 30,
            reserve_a: None,
            reserve_b: None,
            factory: String::new(),
            dex_type: "stable".into(),
        };
        apply_summaries_with_edges(&mut response, &[summary], &[edge]);
        assert_eq!(response.venues[0].source, "unitflow");
        assert_eq!(response.venues[0].swap_participation, 1);
        assert_eq!(response.venues.last().unwrap().source, "unattributed");
    }
}
