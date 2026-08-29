//! Quote engine: the main orchestrator that ties together path finding,
//! quoting, and split optimization.

use {
    crate::{
        path_finder::{PathFinder, PathFinderConfig},
        split_optimizer::{QuotedPath, SplitConfig, SplitOptimizer},
        types::*,
    },
    dex_adapters::{
        clmm_math::{self, ClmmCoverageInput, ClmmPoolState, TickDataStore},
        DexAdapter,
    },
    market_snapshot::{
        decimals,
        pool_state_store::{FactoryRecord, StablePoolStateValue, XykPoolStateValue},
        ClmmCoverageSnapshot, MarketSnapshot,
    },
    std::{collections::HashMap, sync::Arc},
    tokio::sync::RwLock,
    tracing::{debug, info, warn},
};

const CLASSIC_SOURCE: &str = "classic_dex";

#[derive(Debug, Clone)]
pub struct SnapshotClmmQuoteState {
    pub source: String,
    pub pool_address: String,
    pub is_complete: bool,
    pub pool: ClmmPoolState,
    pub ticks: TickDataStore,
    pub coverage: Option<ClmmCoverageSnapshot>,
}

fn clmm_coverage_input(state: &SnapshotClmmQuoteState) -> ClmmCoverageInput {
    if let Some(coverage) = &state.coverage {
        return ClmmCoverageInput::from_snapshot(&state.pool, coverage);
    }
    let range = clmm_math::loaded_tick_range(&state.ticks, state.pool.tick_spacing);
    ClmmCoverageInput {
        pool_tick: state.pool.tick,
        tick_spacing: state.pool.tick_spacing,
        is_complete: state.is_complete,
        min_loaded_tick: range.map(|(min_tick, _)| min_tick),
        max_loaded_tick: range.map(|(_, max_tick)| max_tick),
        scanned_word_start: None,
        scanned_word_end: None,
    }
}

fn apply_slippage(amount: u128, slippage_bps: u32) -> u128 {
    amount * (10_000 - slippage_bps as u128) / 10_000
}

/// Skip xy=k pools with dust reserves on either side (misleading quotes at
/// small trade sizes).
const MIN_XYK_RESERVE_atomic unitsS: u128 = 100_000_000;

fn reserves_for_edge(token_in: &TokenId, token_out: &TokenId, hydrated: &XykPoolStateValue) -> Option<(u128, u128)> {
    let in_key = token_in.canonical();
    let out_key = token_out.canonical();
    if in_key == hydrated.token_a && out_key == hydrated.token_b {
        Some((hydrated.reserve_a, hydrated.reserve_b))
    } else if in_key == hydrated.token_b && out_key == hydrated.token_a {
        Some((hydrated.reserve_b, hydrated.reserve_a))
    } else {
        None
    }
}

/// Per-request pool state overlay (Redis hydrate + optional RPC fallback). Not
/// cached across quotes.
#[derive(Debug, Clone, Default)]
pub struct QuoteHydration {
    pub xyk_pools: HashMap<String, XykPoolStateValue>,
    pub clmm_pools: HashMap<String, SnapshotClmmQuoteState>,
    /// Chakra stableswap pools keyed by `source:pool_address`. The Xylo and
    /// Presto spoke state also lives here (stable-family venues).
    pub stable_pools: HashMap<String, StablePoolStateValue>,
    /// Allowlisted venue factories (from `chakra:factories`). Empty list =
    /// accept legacy unstamped pools.
    pub factories: Vec<FactoryRecord>,
}

impl QuoteHydration {
    pub fn xyk_pool_key(source: &str, pool_address: &str) -> String {
        XykPoolStateValue::pool_key(source, pool_address)
    }

    pub fn clmm_pool_key(source: &str, pool_address: &str) -> String {
        format!("{source}:{pool_address}")
    }

    pub fn stable_pool_key(source: &str, pool_address: &str) -> String {
        StablePoolStateValue::pool_key(source, pool_address)
    }

    /// T4.5: Check if a pool is allowlisted via `chakra:factories`.
    /// Empty factories list = accept all (legacy mode).
    /// Non-empty list + pool's stamped factory matches an allowlisted record = allow.
    /// Non-empty list + no matching factory record = skip.
    pub fn factory_allows_pool(&self, pool_factory: &str) -> bool {
        if self.factories.is_empty() {
            return true;
        }
        if pool_factory.is_empty() {
            // Legacy pool without factory stamp: accept only when factories
            // are empty (already handled above).
            return false;
        }
        self.factories
            .iter()
            .any(|r| r.address.eq_ignore_ascii_case(pool_factory))
    }
}

/// The main quote engine that coordinates all routing logic.
pub struct QuoteEngine {
    path_finder: RwLock<PathFinder>,
    split_optimizer: SplitOptimizer,
    adapters: RwLock<Vec<Arc<dyn DexAdapter>>>,
    /// All cached pool edges (one entry per token pair per pool; same pool may
    /// appear many times).
    cached_pools: RwLock<Vec<TradingPair>>,
    clmm_quote_states: RwLock<HashMap<String, SnapshotClmmQuoteState>>,
}

impl QuoteEngine {
    pub fn new(path_finder_config: PathFinderConfig, split_config: SplitConfig) -> Self {
        Self {
            path_finder: RwLock::new(PathFinder::new(path_finder_config)),
            split_optimizer: SplitOptimizer::new(split_config),
            adapters: RwLock::new(Vec::new()),
            cached_pools: RwLock::new(Vec::new()),
            clmm_quote_states: RwLock::new(HashMap::new()),
        }
    }

    fn clmm_quote_key(source: &str, pool_address: &str) -> String {
        format!("{source}:{pool_address}")
    }

    pub async fn update_clmm_quote_state(
        &self,
        source: &str,
        pool_address: &str,
        pool: ClmmPoolState,
        ticks: TickDataStore,
        is_complete: bool,
        coverage: Option<ClmmCoverageSnapshot>,
    ) {
        self.clmm_quote_states.write().await.insert(
            Self::clmm_quote_key(source, pool_address),
            SnapshotClmmQuoteState {
                source: source.to_string(),
                pool_address: pool_address.to_string(),
                is_complete,
                pool,
                ticks,
                coverage,
            },
        );
    }

    fn find_pool_edge<'a>(
        pools: &'a [TradingPair],
        pool_address: &str,
        token_in: &TokenId,
        token_out: &TokenId,
    ) -> Option<&'a TradingPair> {
        let in_key = token_in.canonical();
        let out_key = token_out.canonical();
        pools.iter().find(|p| {
            p.pool_address == pool_address
                && ((p.token_a.canonical() == in_key && p.token_b.canonical() == out_key)
                    || (p.token_b.canonical() == in_key && p.token_a.canonical() == out_key))
        })
    }

    /// Register a new DEX adapter and update the token graph.
    pub async fn register_adapter(&self, adapter: Arc<dyn DexAdapter>) {
        let source = adapter.id().to_string();
        info!(source = %source, "Registering DEX adapter");

        // Fetch trading pairs from the adapter
        match adapter.get_trading_pairs().await {
            Ok(pairs) => {
                let trading_pairs: Vec<TradingPair> = pairs
                    .into_iter()
                    .map(|p| TradingPair {
                        token_a: p.token_a,
                        token_b: p.token_b,
                        source: source.clone(),
                        pool_address: p.pool_address,
                        fee_bps: p.fee_bps,
                        reserve_a: p.reserve_a,
                        reserve_b: p.reserve_b,
                        factory: String::new(),
                        dex_type: String::new(),
                    })
                    .collect();

                {
                    let mut pf = self.path_finder.write().await;
                    pf.update_from_source(&source, &trading_pairs);
                }
                {
                    let mut cache = self.cached_pools.write().await;
                    cache.retain(|p| p.source != source);
                    cache.extend(trading_pairs.iter().cloned());
                }

                info!(
                    source = %source,
                    pairs = trading_pairs.len(),
                    "Adapter registered successfully"
                );
            }
            Err(e) => {
                warn!(source = %source, error = %e, "Failed to fetch pairs from adapter");
            }
        }

        self.adapters.write().await.push(adapter);
    }

    /// Register an adapter used only for on-chain `get_quote` (no graph
    /// refresh). Snapshot mode attaches CLMM adapters this way while the
    /// graph stays on Redis snapshots.
    pub async fn register_quote_adapter(&self, adapter: Arc<dyn DexAdapter>) {
        info!(source = %adapter.id(), "Registering quote-only DEX adapter");
        self.adapters.write().await.push(adapter);
    }

    /// Update the path finder directly from cached pairs (no RPC needed).
    /// Used for instant startup from disk cache.
    /// Also stores pairs in cached_pools for local quote computation.
    pub async fn update_pairs_from_cache(&self, source: &str, pairs: &[TradingPair]) {
        {
            let mut pf = self.path_finder.write().await;
            pf.update_from_source(source, pairs);
        }

        let mut cache = self.cached_pools.write().await;
        cache.retain(|p| p.source != source);
        cache.extend(pairs.iter().cloned());

        info!(source = source, pairs = pairs.len(), "Path finder updated from cache");
    }

    /// Update the engine from a Chakra topology snapshot (T4.2 test helper;
    /// API wiring is T4.3). Pairs outside the v1 catalog are dropped by
    /// `pairs_from_chakra_snapshot`; each source's edges keep their real
    /// source name (`chakra-xyk`, `xylo-stable`, `presto-hub`, …) so hop
    /// dispatch and hydration keys match.
    pub async fn update_from_chakra_snapshot(&self, snapshot: &MarketSnapshot) {
        let mut by_source: std::collections::BTreeMap<String, Vec<TradingPair>> = Default::default();
        for pair in crate::path_finder::pairs_from_chakra_snapshot(snapshot) {
            by_source.entry(pair.source.clone()).or_default().push(pair);
        }
        for (source, pairs) in by_source {
            self.update_pairs_from_cache(&source, &pairs).await;
        }
    }

    /// Remove a DEX adapter.
    pub async fn unregister_adapter(&self, adapter_id: &str) {
        self.adapters.write().await.retain(|a| a.id() != adapter_id);

        let mut pf = self.path_finder.write().await;
        pf.clear_cache();
    }

    /// Snapshot of graph edges (for hydrate RPC orientation).
    pub async fn cached_pool_edges(&self) -> Vec<TradingPair> {
        self.cached_pools.read().await.clone()
    }

    /// Get all unique token addresses known to the engine.
    pub async fn get_all_tokens(&self) -> Vec<String> {
        let pools = self.cached_pools.read().await;
        let mut tokens: std::collections::HashSet<String> = std::collections::HashSet::new();
        for pair in pools.iter() {
            tokens.insert(pair.token_a.canonical());
            tokens.insert(pair.token_b.canonical());
        }
        let mut result: Vec<String> = tokens.into_iter().collect();
        result.sort();
        result
    }

    /// Discover candidate paths for a route request (graph only).
    /// Quote a single path at `amount_in` using Redis hydration (no split
    /// optimizer).
    pub async fn quote_path_at_amount(
        &self,
        path: &Path,
        amount_in: u128,
        hydration: Option<&QuoteHydration>,
    ) -> Option<Quote> {
        self.quote_path(path, amount_in, hydration).await
    }

    pub async fn find_candidate_paths(&self, request: &RouteRequest) -> Vec<Path> {
        let (max_hops, max_multi_hop_paths, max_direct_paths) = {
            let pf = self.path_finder.read().await;
            (
                request.max_hops.unwrap_or(pf.default_max_hops()),
                pf.default_max_multi_hop_paths(),
                pf.default_max_direct_paths(),
            )
        };
        let pf = self.path_finder.read().await;
        pf.find_paths_with_limits(
            &request.token_in,
            &request.token_out,
            max_hops,
            max_multi_hop_paths,
            max_direct_paths,
        )
    }

    /// Get the optimal route for a trade.
    pub async fn get_route(&self, request: &RouteRequest) -> OptimalRoute {
        let paths = self.find_candidate_paths(request).await;
        self.get_route_with_paths(request, &paths, None).await
    }

    /// Quote using pre-discovered paths and optional per-request pool state
    /// hydration.
    pub async fn get_route_with_paths(
        &self,
        request: &RouteRequest,
        paths: &[Path],
        hydration: Option<&QuoteHydration>,
    ) -> OptimalRoute {
        let start = std::time::Instant::now();
        let slippage_bps = request.slippage_bps.unwrap_or(50);

        // SC-12: native USDC (gas) must never be a swap amount. Reject the
        // request before any path discovery / quoting.
        if decimals::is_native_usdc_encoding(&request.token_in.canonical())
            || decimals::is_native_usdc_encoding(&request.token_out.canonical())
        {
            return OptimalRoute {
                sub_orders: vec![],
                total_amount_in: request.amount_in,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                protocol_fee_bps: 0,
                minimum_out: 0,
                compute_time_ms: start.elapsed().as_millis() as u64,
                debug: None,
            };
        }

        // Never return mixed classic+Arc hops — they cannot form one tx.
        // prefer_arc: Arc AMMs only (no classic / no Horizon).
        // Default: pure classic OR pure Arc compete; Horizon only for classic.
        let prefer_arc = request.prefer_arc.unwrap_or(false);
        let paths: Vec<Path> = paths
            .iter()
            .filter(|path| {
                if prefer_arc {
                    !Self::path_contains_classic(path)
                } else {
                    Self::path_is_executable(path)
                }
            })
            .cloned()
            .collect();

        if paths.is_empty() {
            debug!(
                token_in = %request.token_in,
                token_out = %request.token_out,
                "No paths found"
            );
            return OptimalRoute {
                sub_orders: vec![],
                total_amount_in: request.amount_in,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                protocol_fee_bps: 0,
                minimum_out: 0,
                compute_time_ms: start.elapsed().as_millis() as u64,
                debug: None,
            };
        }

        debug!(paths = paths.len(), "Paths discovered");

        // 2. Get quotes for each path at full amount (parallel — paths are
        //    independent).
        let hydration_owned = hydration.cloned();
        let amount_in = request.amount_in;
        let quoted_paths: Vec<QuotedPath> = futures::future::join_all(paths.iter().map(|path| {
            let path = path.clone();
            let hydration = hydration_owned.clone();
            async move {
                self.quote_path(&path, amount_in, hydration.as_ref())
                    .await
                    .map(|quote| QuotedPath { path, quote })
            }
        }))
        .await
        .into_iter()
        .flatten()
        .collect();

        if quoted_paths.is_empty() {
            return OptimalRoute {
                sub_orders: vec![],
                total_amount_in: request.amount_in,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                protocol_fee_bps: 0,
                minimum_out: 0,
                compute_time_ms: start.elapsed().as_millis() as u64,
                debug: None,
            };
        }

        debug!(quoted = quoted_paths.len(), "Paths quoted");

        let (classic_quoted_paths, Arc_quoted_paths): (Vec<QuotedPath>, Vec<QuotedPath>) = quoted_paths
            .into_iter()
            .partition(|quoted| Self::is_classic_only_path(&quoted.path));

        let best_classic_route = classic_quoted_paths
            .iter()
            .max_by_key(|quoted| quoted.quote.amount_out)
            .map(|quoted| {
                let minimum_out = apply_slippage(quoted.quote.amount_out, slippage_bps);
                OptimalRoute {
                    sub_orders: vec![SubOrder {
                        path: quoted.path.clone(),
                        amount_in: request.amount_in,
                        expected_amount_out: quoted.quote.amount_out,
                        fraction: 1.0,
                    }],
                    total_amount_in: request.amount_in,
                    total_expected_out: quoted.quote.amount_out,
                    price_impact_bps: quoted.quote.price_impact_bps,
                    is_split: false,
                    improvement_bps: 0,
                    protocol_fee_bps: 0,
                    minimum_out,
                    compute_time_ms: start.elapsed().as_millis() as u64,
                    debug: None,
                }
            });

        let best_Arc_route = if Arc_quoted_paths.is_empty() {
            None
        } else {
            let hydration_owned = hydration.cloned();
            let quote_cache = std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::<
                (String, u128),
                Option<Quote>,
            >::new()));
            let amount_bucket = (request.amount_in / 512).max(1);
            Some(
                self.split_optimizer
                    .optimize(
                        &Arc_quoted_paths,
                        request.amount_in,
                        slippage_bps,
                        request.max_splits,
                        |path, amount| {
                            let path_clone = path.clone();
                            let hydration_ref = hydration_owned.as_ref();
                            let cache = quote_cache.clone();
                            async move {
                                let bucket = if amount == 0 {
                                    0
                                } else {
                                    (amount / amount_bucket) * amount_bucket
                                };
                                let key = (path_clone.pool_addresses.join("+"), bucket);
                                if let Some(cached) = cache.lock().await.get(&key) {
                                    if cached.as_ref().is_some_and(|q| q.amount_in == amount) {
                                        return cached.clone();
                                    }
                                }
                                let quote = self.quote_path(&path_clone, amount, hydration_ref).await;
                                cache.lock().await.insert(key, quote.clone());
                                quote
                            }
                        },
                    )
                    .await,
            )
        };

        match (best_classic_route, best_Arc_route) {
            (Some(classic), Some(Arc)) => {
                if classic.total_expected_out > Arc.total_expected_out {
                    classic
                } else {
                    Arc
                }
            }
            (Some(classic), None) => classic,
            (None, Some(Arc)) => Arc,
            (None, None) => OptimalRoute {
                sub_orders: vec![],
                total_amount_in: request.amount_in,
                total_expected_out: 0,
                price_impact_bps: 0,
                is_split: false,
                improvement_bps: 0,
                protocol_fee_bps: 0,
                minimum_out: 0,
                compute_time_ms: start.elapsed().as_millis() as u64,
                debug: None,
            },
        }
    }

    fn is_classic_only_path(path: &Path) -> bool {
        !path.sources.is_empty() && path.sources.iter().all(|source| source == CLASSIC_SOURCE)
    }

    fn path_contains_classic(path: &Path) -> bool {
        path.sources.iter().any(|source| source == CLASSIC_SOURCE)
    }

    /// Pure classic or pure Arc — never mixed hops (unexecutable as one tx).
    fn path_is_executable(path: &Path) -> bool {
        Self::is_classic_only_path(path) || !Self::path_contains_classic(path)
    }

    /// Quote a single path by simulating each hop sequentially.
    /// Uses local AMM computation from Redis hydration + cached reserves.
    async fn quote_path(&self, path: &Path, amount_in: u128, hydration: Option<&QuoteHydration>) -> Option<Quote> {
        let mut current_amount = amount_in;
        let mut total_fee_bps: u32 = 0;
        let mut max_impact_bps: u32 = 0;
        let cached_pools = self.cached_pools.read().await;
        let clmm_quote_states = self.clmm_quote_states.read().await;

        for (i, source) in path.sources.iter().enumerate() {
            let token_in = &path.tokens[i];
            let token_out = &path.tokens[i + 1];
            let pool_address = &path.pool_addresses[i];

            // T4.5: Factory gate — skip chakra-* hops whose pool's stamped
            // factory does not match an allowlisted factory record.
            // Non-chakra sources are not gated.
            if source.starts_with("chakra-") {
                let pool_factory = cached_pools
                    .iter()
                    .find(|p| p.source == *source && p.pool_address == *pool_address)
                    .map(|p| p.factory.as_str())
                    .unwrap_or("");
                let factory_ok = hydration.map_or(true, |h| h.factory_allows_pool(pool_factory));
                if !factory_ok {
                    return None;
                }
            }

            // CLMM: local math only during routing (fast). No per-hop RPC simulate.
            let hop_result = if matches!(source.as_str(), "sushi" | "Arc venue_clmm" | "chakra-clmm") {
                self.local_clmm_quote(
                    token_in,
                    token_out,
                    current_amount,
                    pool_address,
                    source,
                    &clmm_quote_states,
                    hydration,
                )
            } else if source == "chakra-stable" {
                self.local_stable_quote(token_in, token_out, current_amount, pool_address, source, hydration)
            } else if matches!(source.as_str(), "xylo" | "xylo-stable") {
                self.local_xylo_quote(token_in, token_out, current_amount, pool_address, source, hydration)
            } else if source == "presto-hub" {
                self.local_presto_quote(token_in, token_out, current_amount, pool_address, source, hydration)
            } else if matches!(source.as_str(), "chakra-xyk" | "unitflow-v25") {
                self.local_evm_xyk_quote(token_in, token_out, current_amount, pool_address, source, hydration)
            } else {
                self.local_quote(
                    token_in,
                    token_out,
                    current_amount,
                    pool_address,
                    source,
                    &cached_pools,
                    &clmm_quote_states,
                    hydration,
                )
            };

            match hop_result {
                Some(hop_quote) => {
                    current_amount = hop_quote.amount_out;
                    total_fee_bps += hop_quote.fee_bps;
                    // Track the maximum per-hop impact (dominates the overall impact)
                    if hop_quote.price_impact_bps > max_impact_bps {
                        max_impact_bps = hop_quote.price_impact_bps;
                    }
                }
                None => return None,
            }
        }

        Some(Quote {
            source: path.sources.join("+"),
            pool_address: path.pool_addresses.join("+"),
            token_in: path.tokens.first()?.clone(),
            token_out: path.tokens.last()?.clone(),
            amount_in,
            amount_out: current_amount,
            price_impact_bps: max_impact_bps,
            fee_bps: total_fee_bps,
            path: path.tokens[1..path.tokens.len() - 1].to_vec(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        })
    }

    /// Local quote computation using cached reserves and AMM formulas.
    fn local_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
        source: &str,
        cached_pools: &[TradingPair],
        clmm_quote_states: &HashMap<String, SnapshotClmmQuoteState>,
        hydration: Option<&QuoteHydration>,
    ) -> Option<dex_adapters::AdapterQuote> {
        if let Some(clmm_quote) = self.local_clmm_quote(
            token_in,
            token_out,
            amount_in,
            pool_address,
            source,
            clmm_quote_states,
            hydration,
        ) {
            return Some(clmm_quote);
        }

        let pair = Self::find_pool_edge(cached_pools, pool_address, token_in, token_out)?;
        let hydrated = hydration.and_then(|h| h.xyk_pools.get(&QuoteHydration::xyk_pool_key(source, pool_address)));
        // Prefer live Redis fee when present — topology snapshot can lag (e.g.
        // Arc venue total_fee_bps misparsed as default 30 while on-chain is 50).
        let fee_bps = hydrated.map(|h| h.fee_bps).unwrap_or(pair.fee_bps);

        let (reserve_in, reserve_out) = if let Some(hydrated) = hydrated {
            reserves_for_edge(token_in, token_out, hydrated)?
        } else if token_in.canonical() == pair.token_a.canonical() {
            (pair.reserve_a?, pair.reserve_b?)
        } else if token_in.canonical() == pair.token_b.canonical() {
            (pair.reserve_b?, pair.reserve_a?)
        } else {
            return None;
        };

        if reserve_in == 0
            || reserve_out == 0
            || reserve_in < MIN_XYK_RESERVE_atomic unitsS
            || reserve_out < MIN_XYK_RESERVE_atomic unitsS
        {
            return None;
        }

        // Apply appropriate AMM formula based on source
        let (amount_out, fee_bps) = match source {
            "Arc venue" => {
                // Arc venue: fee = ceil(amount_in * 3 / 1000)
                let fee = (amount_in * 3 + 999) / 1000;
                let in_after_fee = amount_in - fee;
                let out = in_after_fee * reserve_out / (reserve_in + in_after_fee);
                (out, 30u32)
            }
            "Arc venue" => {
                // Arc venue: fee on output
                let gross = amount_in * reserve_out / (reserve_in + amount_in);
                let commission = gross * fee_bps as u128 / 10_000;
                (gross - commission, fee_bps)
            }
            _ => {
                // Generic constant product
                let in_after_fee = amount_in * (10_000 - fee_bps as u128) / 10_000;
                let out = in_after_fee * reserve_out / (reserve_in + in_after_fee);
                (out, fee_bps)
            }
        };

        if amount_out == 0 {
            return None;
        }

        // Price impact = 1 - actual_out / ideal_out
        // ideal_out = amount_in * reserve_out / reserve_in (spot price, no slippage)
        let ideal_out = amount_in * reserve_out / reserve_in;
        let price_impact_bps = if ideal_out > 0 && amount_out < ideal_out {
            ((ideal_out - amount_out) * 10_000 / ideal_out) as u32
        } else {
            0
        };

        Some(dex_adapters::AdapterQuote {
            amount_out,
            fee_bps,
            price_impact_bps,
        })
    }

    /// Chakra stableswap hop (SC-2): `evm_quote_math::stable_quote` on the
    /// hydrated stable balances. Never xy=k math, never the Arc generic
    /// 9970/10000.
    fn local_stable_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
        source: &str,
        hydration: Option<&QuoteHydration>,
    ) -> Option<dex_adapters::AdapterQuote> {
        let state = hydration?
            .stable_pools
            .get(&QuoteHydration::stable_pool_key(source, pool_address))?;
        let (i, j) = if token_in.canonical().eq_ignore_ascii_case(&state.token_a)
            && token_out.canonical().eq_ignore_ascii_case(&state.token_b)
        {
            (0, 1)
        } else if token_in.canonical().eq_ignore_ascii_case(&state.token_b)
            && token_out.canonical().eq_ignore_ascii_case(&state.token_a)
        {
            (1, 0)
        } else {
            return None;
        };
        let amount_out = dex_adapters::evm_quote_math::stable_quote(state, i, j, amount_in);
        if amount_out == 0 {
            return None;
        }
        // Impact vs the 1:1 spot price of the equal-decimals stable pool (the
        // 4 bps venue fee is always paid, so impact >= fee_bps).
        let spot_out = amount_in;
        let price_impact_bps = if spot_out > amount_out {
            ((spot_out - amount_out) * 10_000 / spot_out) as u32
        } else {
            0
        };
        Some(dex_adapters::AdapterQuote {
            amount_out,
            fee_bps: state.fee_bps,
            price_impact_bps,
        })
    }

    /// XyloNet hop (T-XYLO): `xylo_quote_with_a` (4 bps fee-on-output) on the
    /// hydrated stored reserves, using the **hydrated on-chain amplification**
    /// (`state.a` from `getAmplificationParameter()`, 2026-08-29). Never the
    /// Chakra stable math (A=100, fee-on-input) — the Xylo venue has a
    /// different swap ABI.
    fn local_xylo_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
        source: &str,
        hydration: Option<&QuoteHydration>,
    ) -> Option<dex_adapters::AdapterQuote> {
        let state = hydration?
            .stable_pools
            .get(&QuoteHydration::stable_pool_key(source, pool_address))?;
        let (reserve_in, reserve_out) = if token_in.canonical().eq_ignore_ascii_case(&state.token_a)
            && token_out.canonical().eq_ignore_ascii_case(&state.token_b)
        {
            (state.balance_a, state.balance_b)
        } else if token_in.canonical().eq_ignore_ascii_case(&state.token_b)
            && token_out.canonical().eq_ignore_ascii_case(&state.token_a)
        {
            (state.balance_b, state.balance_a)
        } else {
            return None;
        };
        let amount_out = dex_adapters::evm_quote_math::xylo_quote_with_a(
            reserve_in,
            reserve_out,
            amount_in,
            state.a,
        );
        if amount_out == 0 {
            return None;
        }
        // Impact vs the on-peg 1:1 spot (equal-decimals stableswap); the
        // 4 bps output fee is always paid.
        let spot_out = amount_in;
        let price_impact_bps = if spot_out > amount_out {
            ((spot_out - amount_out) * 10_000 / spot_out) as u32
        } else {
            0
        };
        Some(dex_adapters::AdapterQuote {
            amount_out,
            fee_bps: state.fee_bps,
            price_impact_bps,
        })
    }

    /// Presto hub hop (2026-08-29): `presto_quote` (normalized hub formula,
    /// 997/1000 pathUSD routing) on the hydrated spoke reserves. The hub is
    /// restricted to USDC/EURC discovery. Presto spoke state is stored in
    /// the stable bucket (StablePoolStateValue with A=1 marker).
    fn local_presto_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
        source: &str,
        hydration: Option<&QuoteHydration>,
    ) -> Option<dex_adapters::AdapterQuote> {
        let state = hydration?
            .stable_pools
            .get(&QuoteHydration::stable_pool_key(source, pool_address))?;
        let (reserve_in, reserve_out) = if token_in.canonical().eq_ignore_ascii_case(&state.token_a)
            && token_out.canonical().eq_ignore_ascii_case(&state.token_b)
        {
            (state.balance_a, state.balance_b)
        } else if token_in.canonical().eq_ignore_ascii_case(&state.token_b)
            && token_out.canonical().eq_ignore_ascii_case(&state.token_a)
        {
            (state.balance_b, state.balance_a)
        } else {
            return None;
        };
        // Presto pathUSD routing: USDC is the path; spoke legs are
        // 997/1000 on the raw reserves (equal 6-dp decimals cancel).
        let amount_out = dex_adapters::evm_quote_math::presto_spoke_quote(
            reserve_in,
            reserve_out,
            amount_in,
        );
        if amount_out == 0 {
            return None;
        }
        let spot_out = amount_in;
        let price_impact_bps = if spot_out > amount_out {
            ((spot_out - amount_out) * 10_000 / spot_out) as u32
        } else {
            0
        };
        Some(dex_adapters::AdapterQuote {
            amount_out,
            fee_bps: state.fee_bps,
            price_impact_bps,
        })
    }

    /// Chakra xy=k hop: Uniswap V2 997/1000 (`evm_quote_math::xyk_quote`) on
    /// the hydrated reserves, with integer price impact.
    fn local_evm_xyk_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
        source: &str,
        hydration: Option<&QuoteHydration>,
    ) -> Option<dex_adapters::AdapterQuote> {
        let hydrated = hydration?
            .xyk_pools
            .get(&QuoteHydration::xyk_pool_key(source, pool_address))?;
        let (reserve_in, reserve_out) = reserves_for_edge(token_in, token_out, hydrated)?;
        if reserve_in == 0
            || reserve_out == 0
            || reserve_in < MIN_XYK_RESERVE_atomic unitsS
            || reserve_out < MIN_XYK_RESERVE_atomic unitsS
        {
            return None;
        }
        let amount_out = dex_adapters::evm_quote_math::xyk_quote(reserve_in, reserve_out, amount_in);
        if amount_out == 0 {
            return None;
        }
        let price_impact_bps =
            dex_adapters::evm_quote_math::price_impact_bps(reserve_in, reserve_out, amount_in, amount_out);
        Some(dex_adapters::AdapterQuote {
            amount_out,
            fee_bps: hydrated.fee_bps,
            price_impact_bps,
        })
    }

    fn local_clmm_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
        source: &str,
        clmm_quote_states: &HashMap<String, SnapshotClmmQuoteState>,
        hydration: Option<&QuoteHydration>,
    ) -> Option<dex_adapters::AdapterQuote> {
        if !matches!(source, "sushi" | "Arc venue_clmm" | "chakra-clmm") {
            return None;
        }

        let key = Self::clmm_quote_key(source, pool_address);
        let state = hydration
            .and_then(|h| h.clmm_pools.get(&key))
            .or_else(|| clmm_quote_states.get(&key))?;
        let coverage = clmm_coverage_input(state);
        let token_in_key = token_in.canonical();
        let token_out_key = token_out.canonical();
        let zero_for_one = if token_in_key == state.pool.token0 && token_out_key == state.pool.token1 {
            true
        } else if token_in_key == state.pool.token1 && token_out_key == state.pool.token0 {
            false
        } else {
            return None;
        };

        if !clmm_math::clmm_swap_allowed(&state.pool, &state.ticks, amount_in, zero_for_one, &coverage) {
            return None;
        }

        let (amount_out, _, _) = clmm_math::simulate_swap(&state.pool, &state.ticks, amount_in, zero_for_one)?;

        let price_impact_bps = if state.pool.liquidity > 0 {
            (amount_in
                .saturating_mul(10_000)
                .saturating_div(2 * state.pool.liquidity))
            .min(10_000) as u32
        } else {
            0
        };

        Some(dex_adapters::AdapterQuote {
            amount_out,
            fee_bps: state.pool.fee_bps,
            price_impact_bps,
        })
    }

    /// Get the (in_idx, out_idx) for a pool and swap tokens.
    /// Returns Some((0, 1)) if token_in == token_a && token_out == token_b,
    /// Some((1, 0)) if token_in == token_b && token_out == token_a,
    /// None if pool is unknown or tokens don't match.
    pub async fn get_pool_indices(
        &self,
        pool_address: &str,
        token_in: &TokenId,
        token_out: &TokenId,
    ) -> Option<(u32, u32)> {
        let in_key = token_in.canonical();
        let pools = self.cached_pools.read().await;
        let pair = Self::find_pool_edge(&pools, pool_address, token_in, token_out)?;
        if in_key == pair.token_a.canonical() {
            Some((0, 1))
        } else {
            Some((1, 0))
        }
    }

    /// Refresh trading pairs from all adapters.
    pub async fn refresh_pairs(&self) {
        let adapters = self.adapters.read().await;
        for adapter in adapters.iter() {
            let source = adapter.id().to_string();
            match adapter.get_trading_pairs().await {
                Ok(pairs) => {
                    let trading_pairs: Vec<TradingPair> = pairs
                        .into_iter()
                        .map(|p| TradingPair {
                            token_a: p.token_a,
                            token_b: p.token_b,
                            source: source.clone(),
                            pool_address: p.pool_address,
                            fee_bps: p.fee_bps,
                            reserve_a: p.reserve_a,
                            reserve_b: p.reserve_b,
                            factory: String::new(),
                            dex_type: String::new(),
                        })
                        .collect();

                    let mut pf = self.path_finder.write().await;
                    pf.update_from_source(&source, &trading_pairs);
                }
                Err(e) => {
                    warn!(source = %source, error = %e, "Failed to refresh pairs");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        dex_adapters::clmm_math::{
            bitmap, sqrt_ratio_at_tick, ClmmPoolState, TickDataStore, TickState, TICKS_PER_CHUNK,
        },
        market_snapshot::{
            decimals::{EURC, USDC_ERC20},
            pool_state_store::{StablePoolStateValue, XykPoolStateValue},
            MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
        },
    };

    fn token(id: &str) -> TokenId {
        TokenId::Contract {
            address: id.to_ascii_lowercase(),
        }
    }

    const CIRBTC: &str = "0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF";
    const XYK_POOL_UE: &str = "0x0000000000000000000000000000000000000001";
    const STABLE_POOL_UE: &str = "0x0000000000000000000000000000000000000002";
    const XYK_POOL_UM: &str = "0x0000000000000000000000000000000000000003";

    /// T2.3 seeds (implementation doc): thin xy=k USDC/EURC 10_000e6 per side
    /// (30 bps), deep stable USDC/EURC 200_000e6 per side (A=100, 4 bps).
    const XYK_UE_SEED: u128 = 10_000_000_000;
    const STABLE_UE_SEED: u128 = 200_000_000_000;

    /// The seeded Arc topology (same fixtures as T4.1 PathFinder tests).
    fn chakra_snapshot() -> MarketSnapshot {
        MarketSnapshot::from_sources(
            "chakra-1",
            1_700_000_000_000,
            "arc-testnet",
            vec![
                SourceSnapshot {
                    source: "chakra-xyk".to_string(),
                    pairs: vec![
                        TradingPairSnapshot {
                            token_a: USDC_ERC20.to_string(),
                            token_b: EURC.to_string(),
                            pool_address: XYK_POOL_UE.to_string(),
                            fee_bps: 30,
                            dex_type: "xyk".to_string(),
                            factory: String::new(),
                        },
                        TradingPairSnapshot {
                            token_a: USDC_ERC20.to_string(),
                            token_b: CIRBTC.to_string(),
                            pool_address: XYK_POOL_UM.to_string(),
                            fee_bps: 30,
                            dex_type: "xyk".to_string(),
                            factory: String::new(),
                        },
                    ],
                },
                SourceSnapshot {
                    source: "chakra-stable".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: USDC_ERC20.to_string(),
                        token_b: EURC.to_string(),
                        pool_address: STABLE_POOL_UE.to_string(),
                        fee_bps: 4,
                        dex_type: "stable".to_string(),
                        factory: String::new(),
                    }],
                },
            ],
        )
    }

    /// Build an engine from the seeded Chakra topology with hydrated pool
    /// state (no RPC): xy=k reserves, stable balances, optional CLMM quote
    /// state.
    async fn engine_with_hydration(
        stable: Option<StablePoolStateValue>,
        clmm: Option<(String, SnapshotClmmQuoteState)>,
    ) -> OptimalRoute {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine.update_from_chakra_snapshot(&chakra_snapshot()).await;

        let mut xyk = HashMap::new();
        xyk.insert(
            QuoteHydration::xyk_pool_key("chakra-xyk", XYK_POOL_UE),
            XykPoolStateValue::new(
                "chakra-xyk",
                XYK_POOL_UE,
                USDC_ERC20,
                EURC,
                30,
                XYK_UE_SEED,
                XYK_UE_SEED,
            ),
        );
        xyk.insert(
            QuoteHydration::xyk_pool_key("chakra-xyk", XYK_POOL_UM),
            XykPoolStateValue::new(
                "chakra-xyk",
                XYK_POOL_UM,
                USDC_ERC20,
                CIRBTC,
                30,
                50_000_000_000,
                100_000_000,
            ),
        );

        let mut stable_pools = HashMap::new();
        if let Some(stable) = stable {
            let key = StablePoolStateValue::pool_key(&stable.source, &stable.pool_address);
            stable_pools.insert(key, stable);
        }

        let mut clmm_pools = HashMap::new();
        if let Some((key, state)) = clmm {
            clmm_pools.insert(key, state);
        }

        let hydration = QuoteHydration {
            xyk_pools: xyk,
            clmm_pools,
            stable_pools,
            ..Default::default()
        };

        engine
            .get_route_with_paths(
                &RouteRequest {
                    token_in: token(USDC_ERC20),
                    token_out: token(EURC),
                    amount_in: 1_000_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(5),
                    prefer_arc: None,
                },
                &engine
                    .find_candidate_paths(&RouteRequest {
                        token_in: token(USDC_ERC20),
                        token_out: token(EURC),
                        amount_in: 1_000_000_000,
                        slippage_bps: Some(50),
                        max_hops: Some(1),
                        max_splits: Some(5),
                        prefer_arc: None,
                    })
                    .await,
                Some(&hydration),
            )
            .await
    }

    #[tokio::test]
    async fn quote_hydrates_chakra_stable_and_uses_evm_math() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine.update_from_chakra_snapshot(&chakra_snapshot()).await;

        let stable = StablePoolStateValue::new(
            "chakra-stable",
            STABLE_POOL_UE,
            USDC_ERC20,
            EURC,
            STABLE_UE_SEED,
            STABLE_UE_SEED,
            100,
            4,
        );
        let xyk = XykPoolStateValue::new(
            "chakra-xyk",
            XYK_POOL_UE,
            USDC_ERC20,
            EURC,
            30,
            XYK_UE_SEED,
            XYK_UE_SEED,
        );
        let hydration = QuoteHydration {
            xyk_pools: HashMap::from([(QuoteHydration::xyk_pool_key("chakra-xyk", XYK_POOL_UE), xyk)]),
            stable_pools: HashMap::from([(
                StablePoolStateValue::pool_key("chakra-stable", STABLE_POOL_UE),
                stable.clone(),
            )]),
            ..Default::default()
        };

        let request = RouteRequest {
            token_in: token(USDC_ERC20),
            token_out: token(EURC),
            amount_in: 1_000_000_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(5),
            prefer_arc: None,
        };
        let paths = engine.find_candidate_paths(&request).await;
        let route = engine.get_route_with_paths(&request, &paths, Some(&hydration)).await;

        assert!(!route.is_split, "1_000e6 on deep stable must stay single-path");
        assert_eq!(route.sub_orders.len(), 1);
        assert_eq!(route.sub_orders[0].path.sources, vec!["chakra-stable".to_string()]);
        // T3.2 pinned on-chain vector: 1_000e6 USDC→EURC on the 200k stable pool.
        let expected = dex_adapters::evm_quote_math::stable_quote(&stable, 0, 1, 1_000_000_000);
        assert_eq!(expected, 999_550_535);
        assert_eq!(route.total_expected_out, expected);
        assert_eq!(route.protocol_fee_bps, 0);
    }

    /// SC-2 (T9.2-prep): at `180_000e6` USDC, the thin xy=k pool at small
    /// allocation provides a better marginal rate than the deep stable pool.
    /// The new filter semantic (re-quote at allocated leg size) should allow
    /// this split to pass, and the total output should beat 100% stable by
    /// at least 5 bps.
    #[tokio::test]
    async fn sc2_180k_split_beats_single_stable() {
        const SPLIT_AMOUNT: u128 = 180_000_000_000; // 180_000e6 (SC-2 documented size)
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine.update_from_chakra_snapshot(&chakra_snapshot()).await;

        // Use lowercased EURC to match the pathfinder's normalization
        let eurc_lower = EURC.to_lowercase();
        let stable = StablePoolStateValue::new(
            "chakra-stable",
            STABLE_POOL_UE,
            USDC_ERC20,
            &eurc_lower,
            STABLE_UE_SEED,
            STABLE_UE_SEED,
            100,
            4,
        );
        let xyk = XykPoolStateValue::new(
            "chakra-xyk",
            XYK_POOL_UE,
            USDC_ERC20,
            &eurc_lower,
            30,
            XYK_UE_SEED,
            XYK_UE_SEED,
        );
        let hydration = QuoteHydration {
            xyk_pools: HashMap::from([(QuoteHydration::xyk_pool_key("chakra-xyk", XYK_POOL_UE), xyk)]),
            stable_pools: HashMap::from([(
                StablePoolStateValue::pool_key("chakra-stable", STABLE_POOL_UE),
                stable.clone(),
            )]),
            ..Default::default()
        };

        let request = RouteRequest {
            token_in: token(USDC_ERC20),
            token_out: token(EURC),
            amount_in: SPLIT_AMOUNT,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(5),
            prefer_arc: None,
        };
        let paths = engine.find_candidate_paths(&request).await;
        let route = engine.get_route_with_paths(&request, &paths, Some(&hydration)).await;

        // Control: best single = 100% stable at the same size.
        let best_single = dex_adapters::evm_quote_math::stable_quote(&stable, 0, 1, SPLIT_AMOUNT);

        // The split should beat single stable by at least 5 bps.
        let improvement_bps = ((route.total_expected_out - best_single) * 10_000 / best_single) as u32;
        assert!(
            route.is_split,
            "engine must split at 180_000e6 (improvement {improvement_bps} bps)"
        );
        assert!(
            improvement_bps >= 5,
            "split must beat single stable by >=5 bps, got {improvement_bps} bps (total_out={}, best_single={})",
            route.total_expected_out,
            best_single
        );
        assert!(
            route.sub_orders.len() >= 2,
            "split must have >=2 sub-orders (got {})",
            route.sub_orders.len()
        );
        // Must include both chakra-xyk and chakra-stable legs.
        let sources: Vec<&str> = route.sub_orders.iter().map(|o| o.path.sources[0].as_str()).collect();
        assert!(
            sources.contains(&"chakra-xyk"),
            "split must include chakra-xyk leg, got {:?}",
            sources
        );
        assert!(
            sources.contains(&"chakra-stable"),
            "split must include chakra-stable leg, got {:?}",
            sources
        );
        assert_eq!(route.protocol_fee_bps, 0);
    }

    /// `chakra-clmm` hop with complete coverage quotes locally (SC-1 third
    /// venue); the same pool with incomplete coverage is skipped.
    #[tokio::test]
    async fn chakra_clmm_quotes_when_complete_and_skips_when_incomplete() {
        let (pool, ticks) = sample_clmm_state();
        let complete = SnapshotClmmQuoteState {
            source: "chakra-clmm".to_string(),
            pool_address: "clmm-pool".to_string(),
            is_complete: true,
            pool: pool.clone(),
            ticks: ticks.clone(),
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: true,
                min_loaded_tick: Some(-1000),
                max_loaded_tick: Some(1000),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
        };
        let key = QuoteHydration::clmm_pool_key("chakra-clmm", "clmm-pool");

        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache("chakra-clmm", &[clmm_pair("chakra-clmm", "clmm-pool")])
            .await;
        let route = engine
            .get_route_with_paths(
                &RouteRequest {
                    token_in: token("token-in"),
                    token_out: token("token-out"),
                    amount_in: 1_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(1),
                    prefer_arc: None,
                },
                &engine
                    .find_candidate_paths(&RouteRequest {
                        token_in: token("token-in"),
                        token_out: token("token-out"),
                        amount_in: 1_000_000,
                        slippage_bps: Some(50),
                        max_hops: Some(1),
                        max_splits: Some(1),
                        prefer_arc: None,
                    })
                    .await,
                Some(&QuoteHydration {
                    clmm_pools: HashMap::from([(key.clone(), complete.clone())]),
                    ..Default::default()
                }),
            )
            .await;

        assert_eq!(route.sub_orders.len(), 1, "complete chakra-clmm hop must quote");
        assert_eq!(route.sub_orders[0].path.sources, vec!["chakra-clmm".to_string()]);
        assert!(route.total_expected_out > 0);
        assert_eq!(route.protocol_fee_bps, 0);

        // Incomplete coverage: same pool, `is_complete=false` → hop skipped.
        let incomplete = SnapshotClmmQuoteState {
            is_complete: false,
            coverage: Some(ClmmCoverageSnapshot {
                is_complete: false,
                min_loaded_tick: Some(-1000),
                max_loaded_tick: Some(1000),
                scanned_word_start: None,
                scanned_word_end: None,
            }),
            ..complete
        };
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache("chakra-clmm", &[clmm_pair("chakra-clmm", "clmm-pool")])
            .await;
        let route = engine
            .get_route_with_paths(
                &RouteRequest {
                    token_in: token("token-in"),
                    token_out: token("token-out"),
                    amount_in: 1_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(1),
                    prefer_arc: None,
                },
                &engine
                    .find_candidate_paths(&RouteRequest {
                        token_in: token("token-in"),
                        token_out: token("token-out"),
                        amount_in: 1_000_000,
                        slippage_bps: Some(50),
                        max_hops: Some(1),
                        max_splits: Some(1),
                        prefer_arc: None,
                    })
                    .await,
                Some(&QuoteHydration {
                    clmm_pools: HashMap::from([(key, incomplete)]),
                    ..Default::default()
                }),
            )
            .await;

        assert!(
            route.sub_orders.is_empty(),
            "incomplete chakra-clmm hop must be skipped"
        );
        assert_eq!(route.total_expected_out, 0);
    }

    /// SC-12: native USDC encodings (`native_usdc`, `0x000…0`) are gas only —
    /// a route request using one as token_in or token_out returns an empty
    /// route with zero output.
    #[tokio::test]
    async fn native_usdc_encoding_is_rejected_as_swap_amount() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache(
                "chakra-xyk",
                &[pair_with_tokens(
                    "chakra-xyk",
                    "pool-ue",
                    USDC_ERC20,
                    EURC,
                    XYK_UE_SEED,
                    XYK_UE_SEED,
                )],
            )
            .await;

        for native in [decimals::NATIVE_USDC, "0x0000000000000000000000000000000000000000"] {
            // token_in = native
            let route = engine
                .get_route(&RouteRequest {
                    token_in: token(native),
                    token_out: token(EURC),
                    amount_in: 1_000_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(1),
                    prefer_arc: None,
                })
                .await;
            assert!(route.sub_orders.is_empty(), "native token_in must yield no route");
            assert_eq!(route.total_expected_out, 0);
            assert_eq!(route.protocol_fee_bps, 0);

            // token_out = native
            let route = engine
                .get_route(&RouteRequest {
                    token_in: token(EURC),
                    token_out: token(native),
                    amount_in: 1_000_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(1),
                    prefer_arc: None,
                })
                .await;
            assert!(route.sub_orders.is_empty(), "native token_out must yield no route");
            assert_eq!(route.total_expected_out, 0);
        }
    }

    /// SC-12 / T1.2: USDC (6 dp) → cirBTC (8 dp) output is in cirBTC atomic units
    /// (8 dp range), never 18 dp native wei or 6 dp USDC. The seeded xy=k pool
    /// is 50_000e6 USDC / 1e8 cirBTC (≈1 USDC = 1 cirBTC nominal).
    #[tokio::test]
    async fn usdc_to_cirbtc_output_is_in_cirbtc_8dp_atomic_units() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache(
                "chakra-xyk",
                &[pair_with_tokens(
                    "chakra-xyk",
                    "pool-um",
                    USDC_ERC20,
                    CIRBTC,
                    50_000_000_000, // 50_000e6 USDC
                    100_000_000,    // 1e8 mBTC
                )],
            )
            .await;
        let hydration = QuoteHydration {
            xyk_pools: HashMap::from([(
                QuoteHydration::xyk_pool_key("chakra-xyk", "pool-um"),
                XykPoolStateValue::new(
                    "chakra-xyk",
                    "pool-um",
                    USDC_ERC20,
                    CIRBTC.to_ascii_lowercase(),
                    30,
                    50_000_000_000,
                    100_000_000,
                ),
            )]),
            ..Default::default()
        };

        let request = RouteRequest {
            token_in: token(USDC_ERC20),
            token_out: token(CIRBTC),
            amount_in: 1_000_000_000, // 1_000e6 USDC (6 dp)
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
            prefer_arc: None,
        };
        let paths = engine.find_candidate_paths(&request).await;
        let route = engine.get_route_with_paths(&request, &paths, Some(&hydration)).await;

        assert_eq!(route.sub_orders.len(), 1);
        let out = route.total_expected_out;
        assert!(out > 0, "USDC→mBTC must quote");
        // The seeded pool is 50_000e6 USDC / 1e8 mBTC → spot ≈ 0.002 mBTC/USDC,
        // so 1_000e6 USDC yields ~1.95e6 mBTC **atomic (8 dp)** units. The
        // assertion pins the exact 997/1000 venue output and rejects any
        // 18 dp wei-scale (1e18+) or 6 dp misread.
        let expected = dex_adapters::evm_quote_math::xyk_quote(50_000_000_000, 100_000_000, 1_000_000_000);
        assert_eq!(out, expected);
        assert!(
            out < 10_000_000,
            "mBTC output {out} must stay in 8 dp atomic range (a few mBTC), not 18 dp wei scale"
        );
        assert_eq!(route.protocol_fee_bps, 0);
    }

    fn pair(source: &str, pool: &str, reserve_a: u128, reserve_b: u128) -> TradingPair {
        TradingPair {
            token_a: token("token-in"),
            token_b: token("token-out"),
            source: source.to_string(),
            pool_address: pool.to_string(),
            fee_bps: 0,
            reserve_a: Some(reserve_a),
            reserve_b: Some(reserve_b),
            factory: String::new(),
            dex_type: String::new(),
        }
    }

    fn pair_with_tokens(
        source: &str,
        pool: &str,
        token_a: &str,
        token_b: &str,
        reserve_a: u128,
        reserve_b: u128,
    ) -> TradingPair {
        TradingPair {
            token_a: TokenId::Contract {
                address: token_a.to_ascii_lowercase(),
            },
            token_b: TokenId::Contract {
                address: token_b.to_ascii_lowercase(),
            },
            source: source.to_string(),
            pool_address: pool.to_string(),
            fee_bps: 30,
            reserve_a: Some(reserve_a),
            reserve_b: Some(reserve_b),
            factory: String::new(),
            dex_type: String::new(),
        }
    }

    fn clmm_pair(source: &str, pool: &str) -> TradingPair {
        TradingPair {
            token_a: token("token-in"),
            token_b: token("token-out"),
            source: source.to_string(),
            pool_address: pool.to_string(),
            fee_bps: 30,
            reserve_a: None,
            reserve_b: None,
            factory: String::new(),
            dex_type: String::new(),
        }
    }

    fn sample_clmm_state() -> (ClmmPoolState, TickDataStore) {
        let pool = ClmmPoolState {
            sqrt_price_x96: sqrt_ratio_at_tick(0),
            tick: 0,
            liquidity: 10_000_000_000_000u128,
            fee_bps: 30,
            tick_spacing: 200,
            token0: "token-in".to_string(),
            token1: "token-out".to_string(),
        };
        let mut ticks = TickDataStore::new();
        let lower_compressed = bitmap::compress_tick(-1000, 200);
        let upper_compressed = bitmap::compress_tick(1000, 200);
        let (lower_chunk, lower_slot) = bitmap::chunk_address(lower_compressed);
        let (upper_chunk, upper_slot) = bitmap::chunk_address(upper_compressed);

        let mut lower_chunk_data = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0,
            };
            TICKS_PER_CHUNK as usize
        ];
        lower_chunk_data[lower_slot as usize] = TickState {
            liquidity_gross: 10_000_000_000_000,
            liquidity_net: 10_000_000_000_000,
        };
        ticks.chunks.insert(lower_chunk, lower_chunk_data);

        let mut upper_chunk_data = vec![
            TickState {
                liquidity_gross: 0,
                liquidity_net: 0,
            };
            TICKS_PER_CHUNK as usize
        ];
        upper_chunk_data[upper_slot as usize] = TickState {
            liquidity_gross: 10_000_000_000_000,
            liquidity_net: -10_000_000_000_000,
        };
        ticks.chunks.insert(upper_chunk, upper_chunk_data);

        let (bm_word_lower, bm_bit_lower) = bitmap::chunk_bitmap_position(lower_chunk);
        let (bm_word_upper, bm_bit_upper) = bitmap::chunk_bitmap_position(upper_chunk);
        let mut word = [0u8; 32];
        set_bit_in_word(&mut word, bm_bit_lower);
        set_bit_in_word(&mut word, bm_bit_upper);
        ticks.chunk_bitmap.insert(bm_word_lower, word);
        if bm_word_upper != bm_word_lower {
            let mut word2 = [0u8; 32];
            set_bit_in_word(&mut word2, bm_bit_upper);
            ticks.chunk_bitmap.insert(bm_word_upper, word2);
        }

        let (l2_pos, l2_bit) = bitmap::word_bitmap_position(bm_word_lower);
        let mut l2_word = [0u8; 32];
        set_bit_in_word(&mut l2_word, l2_bit);
        ticks.word_bitmap.insert(l2_pos, l2_word);

        (pool, ticks)
    }

    fn set_bit_in_word(word: &mut [u8; 32], bit_pos: u32) {
        let byte_idx = 31usize - (bit_pos / 8) as usize;
        let bit_idx = (bit_pos % 8) as u8;
        word[byte_idx] |= 1u8 << bit_idx;
    }

    #[tokio::test]
    async fn quote_uses_snapshot_clmm_state_when_reserves_are_missing() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache("sushi", &[clmm_pair("sushi", "sushi-pool")])
            .await;
        let (pool, ticks) = sample_clmm_state();
        engine
            .update_clmm_quote_state(
                "sushi",
                "sushi-pool",
                pool,
                ticks,
                true,
                Some(ClmmCoverageSnapshot {
                    is_complete: true,
                    min_loaded_tick: Some(-1000),
                    max_loaded_tick: Some(1000),
                    scanned_word_start: None,
                    scanned_word_end: None,
                }),
            )
            .await;

        let route = engine
            .get_route(&RouteRequest {
                token_in: token("token-in"),
                token_out: token("token-out"),
                amount_in: 1_000_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
                prefer_arc: None,
            })
            .await;

        assert_eq!(route.sub_orders.len(), 1);
        assert!(route.total_expected_out > 0);
        assert_eq!(route.sub_orders[0].path.sources, vec!["sushi".to_string()]);
    }

    #[tokio::test]
    async fn quote_rejects_snapshot_clmm_state_without_initialized_ticks() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache("sushi", &[clmm_pair("sushi", "sushi-empty")])
            .await;
        engine
            .update_clmm_quote_state(
                "sushi",
                "sushi-empty",
                ClmmPoolState {
                    sqrt_price_x96: sqrt_ratio_at_tick(0),
                    tick: 0,
                    liquidity: 10_000_000_000_000u128,
                    fee_bps: 30,
                    tick_spacing: 200,
                    token0: "token-in".to_string(),
                    token1: "token-out".to_string(),
                },
                TickDataStore::new(),
                true,
                None,
            )
            .await;

        let route = engine
            .get_route(&RouteRequest {
                token_in: token("token-in"),
                token_out: token("token-out"),
                amount_in: 1_000_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
                prefer_arc: None,
            })
            .await;

        assert!(route.sub_orders.is_empty());
        assert_eq!(route.total_expected_out, 0);
    }

    #[tokio::test]
    async fn quote_rejects_clmm_when_active_tick_outside_scanned_window() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache("sushi", &[clmm_pair("sushi", "sushi-window")])
            .await;
        let (pool, ticks) = sample_clmm_state();
        engine
            .update_clmm_quote_state(
                "sushi",
                "sushi-window",
                pool,
                ticks,
                true,
                Some(ClmmCoverageSnapshot {
                    is_complete: true,
                    min_loaded_tick: Some(-1000),
                    max_loaded_tick: Some(1000),
                    scanned_word_start: Some(100),
                    scanned_word_end: Some(101),
                }),
            )
            .await;

        let route = engine
            .get_route(&RouteRequest {
                token_in: token("token-in"),
                token_out: token("token-out"),
                amount_in: 1_000_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
                prefer_arc: None,
            })
            .await;

        assert!(route.sub_orders.is_empty());
    }

    #[tokio::test]
    async fn quote_rejects_incomplete_snapshot_clmm_state() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_pairs_from_cache("sushi", &[clmm_pair("sushi", "sushi-partial")])
            .await;
        let (pool, ticks) = sample_clmm_state();
        engine
            .update_clmm_quote_state(
                "sushi",
                "sushi-partial",
                pool,
                ticks,
                false,
                Some(ClmmCoverageSnapshot {
                    is_complete: false,
                    min_loaded_tick: Some(-1000),
                    max_loaded_tick: Some(1000),
                    scanned_word_start: None,
                    scanned_word_end: None,
                }),
            )
            .await;

        let route = engine
            .get_route(&RouteRequest {
                token_in: token("token-in"),
                token_out: token("token-out"),
                amount_in: 1_000_000,
                slippage_bps: Some(50),
                max_hops: Some(1),
                max_splits: Some(1),
                prefer_arc: None,
            })
            .await;

        assert!(route.sub_orders.is_empty());
        assert_eq!(route.total_expected_out, 0);
    }

    fn chakra_snapshot_with_factories() -> MarketSnapshot {
        MarketSnapshot::from_sources(
            "chakra-1",
            1_700_000_000_000,
            "arc-testnet",
            vec![
                SourceSnapshot {
                    source: "chakra-xyk".to_string(),
                    pairs: vec![
                        TradingPairSnapshot {
                            token_a: USDC_ERC20.to_string(),
                            token_b: EURC.to_string(),
                            pool_address: XYK_POOL_UE.to_string(),
                            fee_bps: 30,
                            dex_type: "xyk".to_string(),
                            factory: "0xaaaa".to_string(),
                        },
                        TradingPairSnapshot {
                            token_a: USDC_ERC20.to_string(),
                            token_b: CIRBTC.to_string(),
                            pool_address: XYK_POOL_UM.to_string(),
                            fee_bps: 30,
                            dex_type: "xyk".to_string(),
                            factory: "0xaaaa".to_string(),
                        },
                    ],
                },
                SourceSnapshot {
                    source: "chakra-stable".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: USDC_ERC20.to_string(),
                        token_b: EURC.to_string(),
                        pool_address: STABLE_POOL_UE.to_string(),
                        fee_bps: 4,
                        dex_type: "stable".to_string(),
                        factory: "0xbbbb".to_string(),
                    }],
                },
            ],
        )
    }

    // ── T4.5: QuoteEngine factory skip ──────────────────────────────

    /// T4.5: Allowlisted stable pool still quotes (control case — same
    /// vector as `quote_hydrates_chakra_stable_and_uses_evm_math`).
    #[tokio::test]
    async fn t45_allowlisted_stable_factory_still_quotes() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_from_chakra_snapshot(&chakra_snapshot_with_factories())
            .await;
        let hydration = QuoteHydration {
            stable_pools: HashMap::from([(
                QuoteHydration::stable_pool_key("chakra-stable", STABLE_POOL_UE),
                StablePoolStateValue::new(
                    "chakra-stable",
                    STABLE_POOL_UE,
                    USDC_ERC20,
                    EURC,
                    STABLE_UE_SEED,
                    STABLE_UE_SEED,
                    100,
                    4,
                ),
            )]),
            factories: vec![FactoryRecord::new("0xbbbb", "stable", "chakra-stable")],
            ..Default::default()
        };
        let route = engine
            .get_route_with_paths(
                &RouteRequest {
                    token_in: token(USDC_ERC20),
                    token_out: token(EURC),
                    amount_in: 1_000_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(1),
                    prefer_arc: None,
                },
                &engine
                    .find_candidate_paths(&RouteRequest {
                        token_in: token(USDC_ERC20),
                        token_out: token(EURC),
                        amount_in: 1_000_000_000,
                        slippage_bps: Some(50),
                        max_hops: Some(1),
                        max_splits: Some(1),
                        prefer_arc: None,
                    })
                    .await,
                Some(&hydration),
            )
            .await;
        assert_eq!(route.total_expected_out, 999_550_535, "allowlisted stable must quote");
    }

    /// T4.5: Unlisted factory pool is skipped (returns empty route).
    #[tokio::test]
    async fn t45_unlisted_factory_pool_is_skipped() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine
            .update_from_chakra_snapshot(&chakra_snapshot_with_factories())
            .await;
        let hydration = QuoteHydration {
            xyk_pools: HashMap::from([(
                QuoteHydration::xyk_pool_key("chakra-xyk", XYK_POOL_UE),
                XykPoolStateValue::new(
                    "chakra-xyk",
                    XYK_POOL_UE,
                    USDC_ERC20,
                    EURC,
                    30,
                    XYK_UE_SEED,
                    XYK_UE_SEED,
                ),
            )]),
            factories: vec![FactoryRecord::new("0xbbbb", "stable", "chakra-stable")],
            ..Default::default()
        };
        let route = engine
            .get_route_with_paths(
                &RouteRequest {
                    token_in: token(USDC_ERC20),
                    token_out: token(EURC),
                    amount_in: 1_000_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(1),
                    prefer_arc: None,
                },
                &engine
                    .find_candidate_paths(&RouteRequest {
                        token_in: token(USDC_ERC20),
                        token_out: token(EURC),
                        amount_in: 1_000_000_000,
                        slippage_bps: Some(50),
                        max_hops: Some(1),
                        max_splits: Some(1),
                        prefer_arc: None,
                    })
                    .await,
                Some(&hydration),
            )
            .await;
        assert!(route.sub_orders.is_empty(), "unlisted factory pool must be skipped");
        assert_eq!(route.total_expected_out, 0);
    }

    /// T4.5: Empty factories list still quotes legacy pools (backward compat).
    #[tokio::test]
    async fn t45_empty_factories_still_quotes_legacy_pools() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine.update_from_chakra_snapshot(&chakra_snapshot()).await;
        let hydration = QuoteHydration {
            xyk_pools: HashMap::from([(
                QuoteHydration::xyk_pool_key("chakra-xyk", XYK_POOL_UE),
                XykPoolStateValue::new(
                    "chakra-xyk",
                    XYK_POOL_UE,
                    USDC_ERC20,
                    EURC,
                    30,
                    XYK_UE_SEED,
                    XYK_UE_SEED,
                ),
            )]),
            stable_pools: HashMap::from([(
                QuoteHydration::stable_pool_key("chakra-stable", STABLE_POOL_UE),
                StablePoolStateValue::new(
                    "chakra-stable",
                    STABLE_POOL_UE,
                    USDC_ERC20,
                    EURC,
                    STABLE_UE_SEED,
                    STABLE_UE_SEED,
                    100,
                    4,
                ),
            )]),
            factories: vec![],
            ..Default::default()
        };
        let route = engine
            .get_route_with_paths(
                &RouteRequest {
                    token_in: token(USDC_ERC20),
                    token_out: token(EURC),
                    amount_in: 1_000_000_000,
                    slippage_bps: Some(50),
                    max_hops: Some(1),
                    max_splits: Some(1),
                    prefer_arc: None,
                },
                &engine
                    .find_candidate_paths(&RouteRequest {
                        token_in: token(USDC_ERC20),
                        token_out: token(EURC),
                        amount_in: 1_000_000_000,
                        slippage_bps: Some(50),
                        max_hops: Some(1),
                        max_splits: Some(1),
                        prefer_arc: None,
                    })
                    .await,
                Some(&hydration),
            )
            .await;
        assert!(
            !route.sub_orders.is_empty(),
            "empty factories must still quote legacy pools"
        );
        assert!(route.total_expected_out > 0);
    }

    // ── T-XYLO: XyloNet hop routing ────────────────────────────

    const XYLO_POOL_UE: &str = "0x0000000000000000000000000000000000000009";
    /// Live XyloNet stored reserves (2026-08-28 same-block probe).
    const XYLO_RESERVE_USDC: u128 = 9_236_986_394_524;
    const XYLO_RESERVE_EURC: u128 = 613_508_500_014;

    fn xylo_snapshot() -> MarketSnapshot {
        MarketSnapshot::from_sources(
            "chakra-xylo-1",
            1_700_000_000_000,
            "arc-testnet",
            vec![
                SourceSnapshot {
                    source: "chakra-stable".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: USDC_ERC20.to_string(),
                        token_b: EURC.to_string(),
                        pool_address: STABLE_POOL_UE.to_string(),
                        fee_bps: 4,
                        dex_type: "stable".to_string(),
                        factory: String::new(),
                    }],
                },
                SourceSnapshot {
                    source: "xylo".to_string(),
                    pairs: vec![TradingPairSnapshot {
                        token_a: USDC_ERC20.to_string(),
                        token_b: EURC.to_string(),
                        pool_address: XYLO_POOL_UE.to_string(),
                        fee_bps: 4,
                        dex_type: "xylo".to_string(),
                        factory: String::new(),
                    }],
                },
            ],
        )
    }

    fn xylo_hydration() -> QuoteHydration {
        QuoteHydration {
            stable_pools: HashMap::from([
                (
                    QuoteHydration::stable_pool_key("chakra-stable", STABLE_POOL_UE),
                    StablePoolStateValue::new(
                        "chakra-stable",
                        STABLE_POOL_UE,
                        USDC_ERC20,
                        EURC,
                        STABLE_UE_SEED,
                        STABLE_UE_SEED,
                        100,
                        4,
                    ),
                ),
                (
                    QuoteHydration::stable_pool_key("xylo", XYLO_POOL_UE),
                    StablePoolStateValue::new(
                        "xylo",
                        XYLO_POOL_UE,
                        USDC_ERC20,
                        EURC,
                        XYLO_RESERVE_USDC,
                        XYLO_RESERVE_EURC,
                        200,
                        4,
                    ),
                ),
            ]),
            ..Default::default()
        }
    }

    /// T-XYLO: at 1e6 USDC→EURC the balanced Chakra stable (0.9996 EURC)
    /// beats the off-peg Xylo (0.8655 EURC) — the router must keep preferring
    /// chakra-stable at small size.
    #[tokio::test]
    async fn xylo_loses_to_chakra_stable_at_small_size() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine.update_from_chakra_snapshot(&xylo_snapshot()).await;

        let request = RouteRequest {
            token_in: token(USDC_ERC20),
            token_out: token(EURC),
            amount_in: 1_000_000,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
            prefer_arc: None,
        };
        let paths = engine.find_candidate_paths(&request).await;
        let route = engine
            .get_route_with_paths(&request, &paths, Some(&xylo_hydration()))
            .await;

        assert_eq!(route.sub_orders.len(), 1);
        assert_eq!(
            route.sub_orders[0].path.sources,
            vec!["chakra-stable".to_string()],
            "small size must prefer the balanced Chakra stable over off-peg Xylo"
        );
        // 1e6 on the 200k stable pool: 999599 (4 bps fee-on-input).
        assert_eq!(route.total_expected_out, 999_599);
    }

    /// T-XYLO: at a size that drains the Chakra stable (~4e6 EURC out), the
    /// deep Xylo pool is the better venue — the router must route through it
    /// (dex_types: ["xylo"]).
    #[tokio::test]
    async fn xylo_wins_at_chakra_capacity_sizes() {
        let engine = QuoteEngine::new(PathFinderConfig::default(), SplitConfig::default());
        engine.update_from_chakra_snapshot(&xylo_snapshot()).await;

        // Chakra stable has 200_000e6 per side; 4_500_000e6 USDC in drains
        // more than half its EURC side (capacity-bound). Xylo has 613_508e6
        // EURC reserves and an A=200 curve — much deeper for EURC out.
        let amount_in = 4_500_000_000_000u128;
        let request = RouteRequest {
            token_in: token(USDC_ERC20),
            token_out: token(EURC),
            amount_in,
            slippage_bps: Some(50),
            max_hops: Some(1),
            max_splits: Some(1),
            prefer_arc: None,
        };
        let paths = engine.find_candidate_paths(&request).await;
        let route = engine
            .get_route_with_paths(&request, &paths, Some(&xylo_hydration()))
            .await;

        let stable_out = dex_adapters::evm_quote_math::stable_quote(
            &xylo_hydration().stable_pools[&QuoteHydration::stable_pool_key("chakra-stable", STABLE_POOL_UE)],
            0,
            1,
            amount_in,
        );
        let xylo_out = dex_adapters::evm_quote_math::xylo_quote(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, amount_in);
        assert!(xylo_out > stable_out, "Xylo must be deeper at capacity size");
        assert_eq!(
            route.sub_orders[0].path.sources,
            vec!["xylo".to_string()],
            "capacity size must route through Xylo"
        );
        assert!(route.total_expected_out > stable_out);
    }
}
