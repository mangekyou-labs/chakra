/**
 * Recent swaps localStorage helper (T6.3).
 *
 * Key: `chakra:recent-swaps:{chainId}:{address}` (address case-insensitive).
 * Newest first, max 20 entries, drop oldest.
 * Explorer URL is derived at render time — never stored.
 */

export const RECENT_SWAPS_MAX = 20;
const STORAGE_PREFIX = 'chakra:recent-swaps';

export interface RecentSwap {
  txHash: string;
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  amountOut: string;
  timestamp: number;
  isSplit: boolean;
}

function storageKey(chainId: number, address: string): string {
  return `${STORAGE_PREFIX}:${chainId}:${address.toLowerCase()}`;
}

function safeStorage(): Storage | null {
  try {
    if (typeof localStorage !== 'undefined') return localStorage;
  } catch {
    // localStorage unavailable (node test env, SSR, etc.)
  }
  return null;
}

/** Derive Arcscan testnet explorer URL for a tx hash. */
export function arcscanTxUrl(txHash: string): string {
  return `https://testnet.arcscan.app/tx/${txHash}`;
}

/** Read recent swaps for a wallet address on a given chain. Newest first. */
export function getRecentSwaps(chainId: number, address: string): RecentSwap[] {
  const storage = safeStorage();
  if (!storage) return [];
  try {
    const raw = storage.getItem(storageKey(chainId, address));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed as RecentSwap[];
  } catch {
    return [];
  }
}

/** Append a swap to recent history. Drops oldest beyond max. */
export function addRecentSwap(
  chainId: number,
  address: string,
  swap: Omit<RecentSwap, 'timestamp'>,
): void {
  const storage = safeStorage();
  if (!storage) return;
  const swaps = getRecentSwaps(chainId, address);
  const entry: RecentSwap = { ...swap, timestamp: Date.now() };
  swaps.unshift(entry);
  if (swaps.length > RECENT_SWAPS_MAX) {
    swaps.length = RECENT_SWAPS_MAX;
  }
  try {
    storage.setItem(storageKey(chainId, address), JSON.stringify(swaps));
  } catch {
    // Storage full or unavailable — silently drop.
  }
}
