/** Formatting helpers for Chakra quote fields (integer bps). */

/** `12` bps → `0.12%`; `0` → `0%`; `4` → `0.04%`. */
export function formatImpactPercent(bps: number): string {
  if (bps <= 0) return '0%';
  const pct = bps / 100;
  return `${pct.toFixed(Math.min(2, pct < 0.01 ? 4 : 2))}%`;
}

/** Protocol fee is 0 on Chakra (SC-13); still render from the API field. */
export function formatProtocolFeePercent(bps: number): string {
  return `${(bps / 100).toFixed(2).replace(/\.?0+$/, '')}%`;
}
