'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  formatBpsPercent,
  formatCompactCount,
  formatMicrosUsd,
  formatRefreshAge,
  microsBigInt,
  parseStatsRange,
  STATS_RANGES,
  statsTokenLabel,
  type StatsRange,
} from '@/lib/stats-format';

type RouteHealth = {
  token_in: string;
  token_out: string;
  direct: boolean;
  multihop: boolean;
  usable_pools: number;
  best_sources: string[];
};

type DailyPoint = { day: string; stablecoin_notional_micros: string; swaps: number };
type VenueRow = {
  source: string;
  label: string;
  swap_participation: number;
  subroutes: number;
  hops: number;
  route_share_bps: number;
  pair_usage: string[];
};

type Stats = {
  meta: {
    chain: string;
    aggregator?: string;
    deployment_block?: number;
    chain_head: number;
    confirmed_head: number;
    indexed_head: number;
    lag_blocks: number;
    freshness_secs: number | null;
    range: string;
    attributed_swaps: number;
    unattributed_swaps: number;
  };
  overview: {
    stablecoin_notional_micros: string;
    confirmed_swaps: number;
    unique_traders: number;
    split_swaps: number;
    split_share_bps: number;
  };
  daily: DailyPoint[];
  venues: VenueRow[];
  route_health: RouteHealth[];
};

function apiBase(): string {
  return (
    process.env.NEXT_PUBLIC_API_BASE_URL ||
    process.env.NEXT_PUBLIC_CHAKRA_API_URL ||
    ''
  ).replace(/\/$/, '');
}

const RANGE_GROUP_LABEL: Record<StatsRange, string> = {
  '14d': 'Last 14 days',
  '30d': 'Last 30 days',
  '90d': 'Last 90 days',
  all: 'All time',
};

const CHART_HEIGHT = 150;
const CHART_WIDTH = 800;
const CHART_PAD = 10;

/** Integer-based chart coordinates: BigInt division first, Number last. */
function chartGeometry(daily: DailyPoint[]): {
  points: string;
  onePoint?: { x: number; y: number };
} {
  if (daily.length === 0) return { points: '' };
  const max = daily.reduce((top, d) => {
    const value = microsBigInt(d.stablecoin_notional_micros);
    return value > top ? value : top;
  }, 0n);
  const vertical = 1_000_000n; // integer resolution before Number conversion
  const coordinates = daily.map((d, index) => {
    const value = microsBigInt(d.stablecoin_notional_micros);
    const x =
      daily.length === 1
        ? CHART_PAD
        : CHART_PAD + Math.round((index / (daily.length - 1)) * (CHART_WIDTH - 2 * CHART_PAD));
    const scaled = max === 0n ? 0n : (value * vertical) / max;
    const y = Math.round(
      CHART_PAD + 2 + (1 - Number(scaled) / Number(vertical)) * (CHART_HEIGHT - 24),
    );
    return { x, y };
  });
  if (coordinates.length === 1) {
    return { points: '', onePoint: coordinates[0] };
  }
  return { points: coordinates.map(({ x, y }) => `${x},${y}`).join(' ') };
}

function SkeletonBar({ className }: { className?: string }) {
  return (
    <div
      aria-hidden
      className={`animate-pulse rounded-md bg-[var(--surface-raised)] motion-reduce:animate-none ${className ?? ''}`}
    />
  );
}

/** Content-shaped loading state (mirrors the overview/chart grid). */
function StatsSkeleton() {
  return (
    <div aria-label="Loading statistics" role="status">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {[0, 1, 2, 3].map((i) => (
          <div className="surface-panel p-4" key={i}>
            <SkeletonBar className="h-3 w-16" />
            <SkeletonBar className="mt-3 h-8 w-28" />
          </div>
        ))}
      </div>
      <div className="surface-panel mt-5 p-5">
        <SkeletonBar className="h-4 w-44" />
        <SkeletonBar className="mt-4 h-40 w-full" />
      </div>
      <div className="mt-5 grid gap-5 lg:grid-cols-2">
        {[0, 1].map((i) => (
          <div className="surface-panel p-5" key={i}>
            <SkeletonBar className="h-4 w-36" />
            <SkeletonBar className="mt-4 h-3 w-full" />
            <SkeletonBar className="mt-2 h-3 w-11/12" />
            <SkeletonBar className="mt-2 h-3 w-4/5" />
          </div>
        ))}
      </div>
      <span className="sr-only">Loading statistics</span>
    </div>
  );
}

function StatusPill({ route }: { route: RouteHealth }) {
  const healthy = route.direct || route.multihop;
  const label = route.direct ? 'Direct' : route.multihop ? 'Multihop' : 'Unavailable';
  return (
    <span
      className={`rounded-full px-2.5 py-0.5 text-xs font-medium tabular-nums ${
        healthy
          ? 'border border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300'
          : 'border border-red-500/20 bg-red-500/10 text-red-500 dark:text-red-300'
      }`}
    >
      {label}
    </span>
  );
}

export default function StatsPage() {
  const [range, setRange] = useState<StatsRange>(() =>
    parseStatsRange(typeof window === 'undefined' ? undefined : window.location.search),
  );
  const [data, setData] = useState<Stats | null>(null);
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const requestId = useRef(0);

  const load = useCallback((target: StatsRange) => {
    const id = ++requestId.current;
    const controller = new AbortController();
    setError(false);
    setLoading(true);
    fetch(`${apiBase()}/api/v1/stats?range=${target}`, { signal: controller.signal })
      .then((response) => {
        if (!response.ok) throw new Error(`stats request failed with ${response.status}`);
        return response.json();
      })
      .then((payload: { data?: Stats }) => {
        if (!payload?.data) throw new Error('empty stats payload');
        if (requestId.current !== id) return; // superseded by a newer range
        setData(payload.data);
      })
      .catch((cause: unknown) => {
        if (requestId.current !== id) return; // stale failure must not clobber
        if (cause instanceof Error && cause.name === 'AbortError') return;
        setError(true);
      })
      .finally(() => {
        if (requestId.current === id) setLoading(false);
      });
    return controller;
  }, []);

  useEffect(() => {
    const controller = load(range);
    return () => controller.abort();
  }, [range, tick, load]);

  // Keep the selected range in the URL (replaceState — no history spam).
  useEffect(() => {
    const url = new URL(window.location.href);
    if (url.searchParams.get('range') !== range) {
      url.searchParams.set('range', range);
      window.history.replaceState(null, '', url);
    }
  }, [range]);

  const geometry = data ? chartGeometry(data.daily) : { points: '' };
  const chartMax = data?.daily.reduce((top, d) => {
    const v = microsBigInt(d.stablecoin_notional_micros);
    return v > top ? v : top;
  }, 0n);

  return (
    <main className="mx-auto w-full max-w-6xl px-5 pb-12 pt-4 sm:px-8">
      <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="section-label">Network analytics</p>
          <h1 className="mt-1 text-3xl font-semibold tracking-tight">Stats</h1>
          <p className="mt-2 text-sm text-[var(--text-secondary)]">
            Confirmed routing activity on Arc Testnet.
          </p>
        </div>
        <div
          role="group"
          aria-label="Date range"
          className="flex rounded-lg border border-[var(--border)] p-1"
        >
          {STATS_RANGES.map((candidate) => (
            <button
              key={candidate}
              type="button"
              aria-pressed={range === candidate}
              aria-label={RANGE_GROUP_LABEL[candidate]}
              onClick={() => setRange(candidate)}
              className={`min-h-10 rounded-md px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] ${
                range === candidate
                  ? 'bg-[var(--accent)] text-[var(--accent-contrast)]'
                  : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
              }`}
            >
              {candidate}
            </button>
          ))}
        </div>
      </div>

      {error && (
        <div
          role="alert"
          className="surface-panel mb-5 flex flex-wrap items-center justify-between gap-4 border-red-500/25 p-4 text-sm"
        >
          <span className="text-red-600 dark:text-red-300">
            Statistics are temporarily unavailable.
          </span>
          <button
            type="button"
            onClick={() => setTick((t) => t + 1)}
            className="min-h-10 rounded-md border border-[var(--border)] px-3 font-medium text-[var(--text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
          >
            Retry
          </button>
        </div>
      )}

      {loading && !data && !error && <StatsSkeleton />}

      {data && (
        <div aria-busy={loading}>
          <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4" aria-label="Overview">
            {(
              [
                ['Notional', formatMicrosUsd(data.overview.stablecoin_notional_micros)],
                ['Confirmed swaps', formatCompactCount(data.overview.confirmed_swaps)],
                ['Unique traders', formatCompactCount(data.overview.unique_traders)],
                ['Split share', formatBpsPercent(data.overview.split_share_bps)],
              ] as const
            ).map(([label, value]) => (
              <div className="surface-panel p-4" key={label}>
                <p className="section-label">{label}</p>
                <p className="mt-2 text-2xl font-[family-name:var(--font-mono)] tabular-nums tracking-tight">
                  {value}
                </p>
              </div>
            ))}
          </section>

          <section className="surface-panel mt-5 p-5" aria-label="Stablecoin notional by day">
            <div className="mb-4 flex flex-wrap items-baseline justify-between gap-2">
              <h2 className="section-title">Stablecoin notional</h2>
              <span className="text-xs text-[var(--text-muted)]">
                USD · {RANGE_GROUP_LABEL[range]}
              </span>
            </div>
            {data.daily.length === 0 ? (
              <p className="flex h-44 items-center justify-center rounded-xl border border-dashed border-[var(--border)] text-sm text-[var(--text-muted)]">
                No confirmed swaps in this range yet.
              </p>
            ) : (
              <svg
                viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
                className="h-44 w-full"
                role="img"
                aria-label="Daily stablecoin notional chart"
              >
                {[0.25, 0.5, 0.75].map((fraction) => {
                  const y = CHART_PAD + 2 + (1 - fraction) * (CHART_HEIGHT - 24);
                  return (
                    <line
                      key={fraction}
                      x1={CHART_PAD}
                      x2={CHART_WIDTH - CHART_PAD}
                      y1={y}
                      y2={y}
                      stroke="var(--border)"
                      strokeDasharray="2 4"
                    />
                  );
                })}
                {geometry.onePoint ? (
                  <circle
                    cx={geometry.onePoint.x}
                    cy={geometry.onePoint.y}
                    r={3.5}
                    fill="var(--accent)"
                    aria-hidden
                  />
                ) : (
                  geometry.points && (
                    <polyline
                      points={geometry.points}
                      fill="none"
                      stroke="var(--accent)"
                      strokeWidth="2.5"
                      strokeLinejoin="round"
                      strokeLinecap="round"
                    />
                  )
                )}
                {chartMax !== undefined && chartMax > 0n && (
                  <text
                    x={CHART_WIDTH - CHART_PAD}
                    y={CHART_HEIGHT - 4}
                    textAnchor="end"
                    fontSize="11"
                    fill="var(--text-muted)"
                  >
                    peak {formatMicrosUsd(chartMax)}
                  </text>
                )}
              </svg>
            )}
          </section>

          <section className="mt-5 grid gap-5 lg:grid-cols-2">
            <div className="surface-panel p-5" aria-label="Venue activity">
              <h2 className="section-title mb-4">Venue activity</h2>
              {data.venues.length === 0 ? (
                <p className="flex h-32 items-center justify-center rounded-xl border border-dashed border-[var(--border)] text-sm text-[var(--text-muted)]">
                  No venue participation in this range.
                </p>
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full min-w-[32rem] text-left text-sm">
                    <thead className="text-xs text-[var(--text-muted)]">
                      <tr>
                        <th scope="col" className="pb-2 font-medium">
                          Venue
                        </th>
                        <th scope="col" className="pb-2 font-medium">
                          Swaps
                        </th>
                        <th scope="col" className="pb-2 font-medium">
                          Subroutes
                        </th>
                        <th scope="col" className="pb-2 font-medium">
                          Hops
                        </th>
                        <th scope="col" className="pb-2 font-medium">
                          Route share
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.venues.map((venue) => (
                        <tr key={venue.source} className="border-t border-[var(--border)]">
                          <td className="py-2 pr-3">{venue.label || venue.source}</td>
                          <td className="py-2 pr-3 font-[family-name:var(--font-mono)] tabular-nums">
                            {formatCompactCount(venue.swap_participation)}
                          </td>
                          <td className="py-2 pr-3 font-[family-name:var(--font-mono)] tabular-nums">
                            {formatCompactCount(venue.subroutes)}
                          </td>
                          <td className="py-2 pr-3 font-[family-name:var(--font-mono)] tabular-nums">
                            {formatCompactCount(venue.hops)}
                          </td>
                          <td className="py-2 font-[family-name:var(--font-mono)] tabular-nums">
                            {formatBpsPercent(venue.route_share_bps)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>

            <div className="surface-panel p-5" aria-label="cirBTC route health">
              <h2 className="section-title mb-1">cirBTC route health</h2>
              <p className="mb-4 text-xs text-[var(--text-muted)]">
                Directed quotes for every USDC / EURC / cirBTC pair · direct or multihop.
              </p>
              {data.route_health.length === 0 ? (
                <p className="flex h-32 items-center justify-center rounded-xl border border-dashed border-[var(--border)] text-sm text-[var(--text-muted)]">
                  Route probes have not reported yet.
                </p>
              ) : (
                <ul className="space-y-2">
                  {data.route_health.map((route) => (
                    <li
                      key={`${route.token_in}-${route.token_out}`}
                      className="flex items-center justify-between gap-3 rounded-lg border border-[var(--border)] px-3 py-2.5 text-sm"
                    >
                      <span className="flex min-w-0 items-center gap-1.5 font-[family-name:var(--font-mono)] tabular-nums">
                        <span className="truncate">{statsTokenLabel(route.token_in)}</span>
                        <span aria-hidden className="text-[var(--text-muted)]">
                          →
                        </span>
                        <span className="truncate">{statsTokenLabel(route.token_out)}</span>
                      </span>
                      <span className="flex shrink-0 items-center gap-2">
                        <span className="hidden text-xs text-[var(--text-muted)] sm:inline">
                          {route.usable_pools > 0
                            ? `${route.usable_pools} pool${route.usable_pools === 1 ? '' : 's'}`
                            : ''}
                        </span>
                        <StatusPill route={route} />
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </section>

          <p className="mt-5 flex flex-wrap gap-x-3 gap-y-1 text-xs text-[var(--text-muted)]">
            <span>
              Indexed head{' '}
              <span className="font-[family-name:var(--font-mono)] tabular-nums">
                {formatCompactCount(data.meta.indexed_head)}
              </span>
            </span>
            <span aria-hidden>·</span>
            <span>
              confirmed{' '}
              <span className="font-[family-name:var(--font-mono)] tabular-nums">
                {formatCompactCount(data.meta.confirmed_head)}
              </span>
            </span>
            <span aria-hidden>·</span>
            <span>
              chain head{' '}
              <span className="font-[family-name:var(--font-mono)] tabular-nums">
                {formatCompactCount(data.meta.chain_head)}
              </span>
            </span>
            <span aria-hidden>·</span>
            <span>
              lag{' '}
              <span className="font-[family-name:var(--font-mono)] tabular-nums">
                {formatCompactCount(data.meta.lag_blocks)}
              </span>{' '}
              blocks
            </span>
            <span aria-hidden>·</span>
            <span>
              {data.meta.freshness_secs === null || data.meta.freshness_secs === undefined
                ? 'freshness unknown'
                : `refreshed ${formatRefreshAge(data.meta.freshness_secs)} ago`}
            </span>
            {data.meta.unattributed_swaps > 0 && (
              <>
                <span aria-hidden>·</span>
                <span>{formatCompactCount(data.meta.unattributed_swaps)} unattributed</span>
              </>
            )}
          </p>
        </div>
      )}
    </main>
  );
}
