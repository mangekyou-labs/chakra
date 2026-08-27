/** Swap token catalog for Chakra (native USDC never a swap token — SC-12). */
import { EURC_ADDRESS, USDC_ERC20_ADDRESS, isNativeSwapToken } from '@/lib/decimals';

export interface SwapToken {
  address: string;
  symbol: string;
  name: string;
  decimals: number;
}

/** Hardcoded fallback catalog when `/tokens` is down. */
export const FALLBACK_SWAP_TOKENS: SwapToken[] = [
  {
    address: USDC_ERC20_ADDRESS,
    symbol: 'USDC',
    name: 'USD Coin',
    decimals: 6,
  },
  {
    address: EURC_ADDRESS,
    symbol: 'EURC',
    name: 'Euro Coin',
    decimals: 6,
  },
];

/** Unknown raw rows (e.g. `native_usdc`, `0x0`, `eth`) are never swap tokens. */
export function filterSwapTokens(
  rows: Array<Partial<SwapToken> & { address?: string }>,
): SwapToken[] {
  const seen = new Set<string>();
  const out: SwapToken[] = [];
  for (const row of rows) {
    const address = (row.address ?? '').toLowerCase();
    if (!address || isNativeSwapToken(address)) continue;
    if (seen.has(address)) continue;
    seen.add(address);
    out.push({
      address,
      symbol: row.symbol ?? '?',
      name: row.name ?? row.symbol ?? '?',
      decimals: typeof row.decimals === 'number' ? row.decimals : 6,
    });
  }
  if (out.length === 0) return FALLBACK_SWAP_TOKENS;
  return out;
}
