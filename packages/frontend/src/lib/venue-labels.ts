const DEX_LABELS: Record<string, string> = {
  xyk: 'xy=k',
  stable: 'Stable',
  clmm: 'CLMM',
  xylo: 'Xylo',
  presto: 'Presto',
  'chakra-xyk': 'xy=k',
  'chakra-stable': 'Stable',
  'chakra-clmm': 'UnitFlow V3',
  'xylo-stable': 'Xylo',
  'presto-hub': 'Presto',
  'unitflow-v25': 'UnitFlow',
};

export function dexLabel(dex: string): string {
  const key = dex.toLowerCase();
  return DEX_LABELS[key] ?? dex.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Translate API source identifiers at every UI summary boundary. */
export function routeSourceLabel(source?: string): string {
  if (!source) return '—';
  return source
    .split('→')
    .map((hop) => dexLabel(hop.trim()))
    .join(' → ');
}
