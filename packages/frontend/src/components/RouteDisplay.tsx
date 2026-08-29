'use client';

import { useEffect, useState } from 'react';
import type { SubRoute } from '@/lib/aggregator';
import { formatImpactPercent } from '@/lib/quote-format';

const DEX_LABELS: Record<string, string> = {
  xyk: 'xy=k',
  stable: 'Stable',
  clmm: 'CLMM',
  xylo: 'Xylo',
  presto: 'Presto',
  'chakra-xyk': 'xy=k',
  'chakra-stable': 'Stable',
  'chakra-clmm': 'CLMM',
  'xylo-stable': 'Xylo',
  'presto-hub': 'Presto',
  'unitflow-v25': 'UnitFlow',
};

function dexLabel(dex: string): string {
  const key = dex.toLowerCase();
  return DEX_LABELS[key] ?? dex.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Per-hop venue list from the Chakra `source` string ("xyk → clmm"). */
export function routeDexHops(route: SubRoute): string[] {
  if (!route.source) return [];
  return route.source
    .split('→')
    .map((s) => s.trim())
    .filter(Boolean);
}

function formatLegAmount(atomic: string, decimals: number): string {
  const value = BigInt(atomic || '0');
  const base = 10n ** BigInt(decimals);
  const whole = value / base;
  const frac = value % base;
  if (frac === 0n) return whole.toString();
  const fracStr = frac.toString().padStart(decimals, '0').replace(/0+$/, '');
  const trimmed = fracStr.length > 6 ? fracStr.slice(0, 6) : fracStr;
  return `${whole}.${trimmed}`;
}

export function RouteDisplay({
  quote,
  tokenInSymbol,
  tokenOutSymbol,
  tokenInDecimals = 7,
  tokenOutDecimals = 7,
  resolveTokenSymbol,
}: {
  quote: import('@/lib/aggregator').QuoteData;
  tokenInSymbol?: string;
  tokenOutSymbol: string;
  tokenInDecimals?: number;
  tokenOutDecimals?: number;
  resolveTokenSymbol: (address: string) => string;
}) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    setOpen(false);
  }, [quote.amount_in, quote.expected_output, quote.sub_routes.length]);

  return (
    <div className="rounded-2xl border border-[var(--border)] bg-[var(--surface)]/80 px-4 py-3 space-y-2.5">
      <div className="flex justify-between text-[13px] sm:text-[14px]">
        <span className="text-[var(--text-muted)]">Price impact</span>
        <span
          className={
            quote.price_impact_bps > 100 ? 'text-amber-400' : 'text-[var(--text-secondary)]'
          }
        >
          {formatImpactPercent(quote.price_impact_bps)}
        </span>
      </div>
      <div className="flex justify-between text-[13px] sm:text-[14px]">
        <span className="text-[var(--text-muted)]">Protocol fee</span>
        <span className="text-[var(--text-secondary)] font-[family-name:var(--font-mono)]">
          {formatImpactPercent(quote.protocol_fee_bps)}
        </span>
      </div>

      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="w-full flex items-center justify-between gap-3 pt-1 text-[13px] sm:text-[14px] hover:opacity-90 transition-opacity"
      >
        <span className="text-[var(--text-muted)]">Route</span>
        <span className="inline-flex items-center gap-2 min-w-0">
          <span className="rounded-full border border-[var(--border)] bg-[var(--bg-0)]/60 px-2.5 py-0.5 text-[12px] font-medium text-[var(--text-secondary)] truncate max-w-[12rem] sm:max-w-[16rem]">
            {quote.sub_routes.length > 1
              ? `${quote.sub_routes.length} paths`
              : (quote.sub_routes[0]?.source ?? '—')}
          </span>
          <svg
            className={`h-3.5 w-3.5 text-[var(--text-muted)] transition-transform ${open ? 'rotate-180' : ''}`}
            fill="none"
            viewBox="0 0 20 20"
            stroke="currentColor"
            aria-hidden
          >
            <path d="m5 7.5 5 5 5-5" strokeWidth="1.75" strokeLinecap="round" />
          </svg>
        </span>
      </button>

      {open && (
        <div className="space-y-2 pt-1 border-t border-[var(--border)]">
          {quote.compute_time_ms > 0 && (
            <div className="text-[12px] text-[var(--text-muted)]">
              Quoted in {quote.compute_time_ms}ms
            </div>
          )}

          <div className="space-y-2">
            {quote.sub_routes.map((route, i) => {
              const hops = routeDexHops(route);
              return (
                <div
                  key={i}
                  className="rounded-xl border border-[var(--border)] bg-[var(--bg-0)]/50 p-3"
                >
                  <div className="flex items-center justify-between gap-2 mb-1.5">
                    <div className="flex flex-wrap items-center gap-1 min-w-0">
                      {hops.map((dex, j) => (
                        <span key={`${dex}-${j}`} className="inline-flex items-center gap-1">
                          {j > 0 && <span className="text-[var(--text-muted)] text-[12px]">→</span>}
                          <span className="text-[13px] font-medium text-[var(--text-secondary)]">
                            {dexLabel(dex)}
                          </span>
                        </span>
                      ))}
                    </div>
                    <div className="text-right shrink-0">
                      <span className="text-[13px] text-[var(--text-muted)] font-[family-name:var(--font-mono)] block">
                        {(route.fraction_bps / 100).toLocaleString(undefined, {
                          maximumFractionDigits: 2,
                        })}
                        %
                      </span>
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5 text-[13px] text-[var(--text-muted)]">
                    <span className="font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
                      {formatLegAmount(route.amount_in, tokenInDecimals)}
                    </span>
                    <span className="text-[var(--text-secondary)] font-medium">
                      {tokenInSymbol ?? resolveTokenSymbol(route.path[0] ?? '')}
                    </span>
                    {route.path.slice(1, -1).map((mid, idx) => (
                      <span key={`${mid}-${idx}`} className="inline-flex items-center gap-1.5">
                        <span className="text-[var(--text-muted)]">→</span>
                        <span className="text-[var(--text-muted)] font-medium">
                          {resolveTokenSymbol(mid)}
                        </span>
                      </span>
                    ))}
                    <span className="text-[var(--text-muted)]">→</span>
                    <span className="font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
                      {formatLegAmount(route.amount_out, tokenOutDecimals)}
                    </span>
                    <span className="text-[var(--text-secondary)] font-medium">
                      {tokenOutSymbol}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
