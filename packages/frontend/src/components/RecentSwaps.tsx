'use client';

import { useMemo } from 'react';
import { getRecentSwaps, arcscanTxUrl, type RecentSwap } from '@/lib/recent-swaps';

type Props = {
  chainId: number;
  address: string | null;
};

/**
 * Recent swaps list under the SwapCard (T6.3).
 * Empty state: "No swaps yet". Rows: tokens, amounts, derived Arcscan link,
 * split badge. All amounts use existing formatters and `font-mono tabular-nums`.
 */
export function RecentSwaps({ chainId, address }: Props) {
  const swaps: RecentSwap[] = useMemo(
    () => (address ? getRecentSwaps(chainId, address) : []),
    [chainId, address],
  );

  if (!address) return null;

  return (
    <div className="surface-panel p-5 sm:p-6">
      <h3 className="text-[14px] font-semibold text-[var(--text-primary)] mb-3">Recent Swaps</h3>
      {swaps.length === 0 ? (
        <p className="text-[13px] text-[var(--text-muted)]">No swaps yet</p>
      ) : (
        <ul className="space-y-2">
          {swaps.map((swap) => (
            <li key={swap.txHash + swap.timestamp} className="group">
              <a
                href={arcscanTxUrl(swap.txHash)}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center justify-between gap-2 py-1.5 px-2 rounded-lg hover:bg-[var(--bg-0)] transition-colors"
              >
                <div className="min-w-0">
                  <span className="text-[13px] font-medium text-[var(--text-primary)] font-[family-name:var(--font-mono)] tabular-nums">
                    {swap.tokenIn} → {swap.tokenOut}
                  </span>
                  {swap.isSplit && (
                    <span className="ml-1.5 inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-[var(--accent)]/10 text-[var(--accent)]">
                      split
                    </span>
                  )}
                </div>
                <span className="text-[12px] text-[var(--text-muted)] font-[family-name:var(--font-mono)] tabular-nums shrink-0 group-hover:text-[var(--accent)]">
                  ↗
                </span>
              </a>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
