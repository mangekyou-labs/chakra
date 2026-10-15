/** Formatting helpers for Chakra quote fields (integer bps). */

/** The API rounds impact down to whole bps; zero means below 0.01%. */
export function formatImpactPercent(bps: number): string {
  if (bps <= 0) return '<0.01%';
  const pct = bps / 100;
  return `${pct.toFixed(Math.min(2, pct < 0.01 ? 4 : 2))}%`;
}

/** Protocol fee is 0 on Chakra (SC-13); still render from the API field. */
export function formatProtocolFeePercent(bps: number): string {
  return `${(bps / 100).toFixed(2).replace(/\.?0+$/, '')}%`;
}

/** Per-path pool fees, in hop order, already deducted from quoted output. */
export function formatPoolFees(routes: { hop_fees?: number[] }[]): string {
  const fees = routes.map((route) =>
    route.hop_fees?.length ? route.hop_fees.map(formatProtocolFeePercent).join(' + ') : '—',
  );
  return [...new Set(fees)].join(' / ') || '—';
}
