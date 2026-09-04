/**
 * Shared formatting for the /stats dashboard (integer-safe, no float money).
 *
 * Monetary fields arrive as integer decimal strings denominated in *micros*
 * (1 USDC / EURC unit = 1,000,000 micros). Every conversion below uses
 * BigInt until the value is small enough to be rendered; chart scaling is
 * integer-based before coordinates become numbers.
 */

import { CIRBTC_ADDRESS, EURC_ADDRESS, USDC_ERC20_ADDRESS } from './decimals';

export const STATS_RANGES = ['14d', '30d', '90d', 'all'] as const;
export type StatsRange = (typeof STATS_RANGES)[number];

const MICROS_PER_USD = 1_000_000n;
const CENTS_PER_USD = 100n;

/** True when `value` is one of the supported /stats ranges. */
export function isStatsRange(value: string | null | undefined): value is StatsRange {
  return (STATS_RANGES as readonly string[]).includes(value ?? '');
}

/** Parse the `?range=` query string (defaults to the documented 30d). */
export function parseStatsRange(search: string | null | undefined): StatsRange {
  const match = search?.match(/[?&]range=([^&#]+)/);
  return isStatsRange(match?.[1]) ? (match[1] as StatsRange) : '30d';
}

/** Group integer digits with thousands separators (no floating point). */
export function groupThousands(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/** Micros → "$1.00". Half-up to the cent, entirely in BigInt arithmetic. */
export function formatMicrosUsd(micros: string | bigint | number): string {
  const value = typeof micros === 'bigint' ? micros : BigInt(micros || '0');
  const roundedCents = (value * CENTS_PER_USD + MICROS_PER_USD / 2n) / MICROS_PER_USD;
  const dollars = roundedCents / CENTS_PER_USD;
  const cents = roundedCents % CENTS_PER_USD;
  return `$${groupThousands(dollars.toString())}.${cents.toString().padStart(2, '0')}`;
}

const USD_UNITS: ReadonlyArray<readonly [bigint, string]> = [
  [1_000_000_000_000n, 'T'],
  [1_000_000_000n, 'B'],
  [1_000_000n, 'M'],
  [1_000n, 'K'],
];

/**
 * Compact dollar value for bounded chart contexts: "$1.2M" instead of
 * "$1,200,000.00". 1,000,000 micros still renders as "$1.00" (never "1M");
 * compacting only applies above $1,000, and the axis uses these labels.
 */
export function formatUsdCompact(micros: string | bigint | number): string {
  const value = typeof micros === 'bigint' ? micros : BigInt(micros || '0');
  const usd = value / MICROS_PER_USD;
  for (const [unit, suffix] of USD_UNITS) {
    if (usd >= unit) {
      const tenths = (usd * 10n) / unit;
      const whole = tenths / 10n;
      const frac = tenths % 10n;
      return `$${groupThousands(whole.toString())}${frac ? `.${frac}` : ''}${suffix}`;
    }
  }
  return formatMicrosUsd(value);
}

/** Compact integer counts: 1,234,567 → "1.2M"; < 1,000 stays grouped. */
export function formatCompactCount(value: number | string): string {
  const n = typeof value === 'string' ? Number(value) : value;
  if (!Number.isFinite(n)) return '—';
  const units: ReadonlyArray<readonly [number, string]> = [
    [1e9, 'B'],
    [1e6, 'M'],
    [1e3, 'K'],
  ];
  for (const [unit, suffix] of units) {
    if (n >= unit) {
      const tenths = Math.round((n / unit) * 10);
      const whole = Math.floor(tenths / 10);
      const frac = tenths % 10;
      return `${groupThousands(whole.toString())}${frac ? `.${frac}` : ''}${suffix}`;
    }
  }
  return groupThousands(Math.round(n).toString());
}

/** Integer bps → percentage: 500 → "5%", 1234 → "12.34%", 5 → "0.05%". */
export function formatBpsPercent(bps: number | string): string {
  const value = typeof bps === 'string' ? Number(bps) : bps;
  if (!Number.isFinite(value) || value <= 0) return '0%';
  const pct = value / 100;
  const rounded = Math.round(pct * 100) / 100;
  return `${rounded.toFixed(2).replace(/\.?0+$/, '')}%`;
}

/** Human age for the freshness footer: 45 → "45s", 130 → "2m 10s". */
export function formatRefreshAge(secs: number): string {
  if (!Number.isFinite(secs) || secs < 0) return '—';
  const total = Math.round(secs);
  if (total < 60) return `${total}s`;
  const minutes = Math.floor(total / 60);
  if (minutes < 60) {
    const rest = total % 60;
    return rest === 0 ? `${minutes}m` : `${minutes}m ${rest}s`;
  }
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  return restMinutes === 0 ? `${hours}h` : `${hours}h ${restMinutes}m`;
}

const SYMBOL_BY_ADDRESS: Record<string, string> = {
  [USDC_ERC20_ADDRESS.toLowerCase()]: 'USDC',
  [EURC_ADDRESS.toLowerCase()]: 'EURC',
  [CIRBTC_ADDRESS.toLowerCase()]: 'cirBTC',
};

/** Short, canonical token label for route-health rows ("cirBTC", not "BTC"). */
export function statsTokenLabel(address: string): string {
  return SYMBOL_BY_ADDRESS[address.toLowerCase()] ?? `${address.slice(0, 6)}…`;
}

/** BigInt parse for a decimal string; never throws on API junk. */
export function microsBigInt(micros: string | bigint | number | null | undefined): bigint {
  if (typeof micros === 'bigint') return micros;
  try {
    return BigInt(micros ?? '0');
  } catch {
    return 0n;
  }
}
