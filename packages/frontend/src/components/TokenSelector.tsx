'use client';

import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { SwapToken } from '@/lib/swap-tokens';
import { useTokenCatalog } from '@/lib/hooks';
import { useAccountBalances } from '@/lib/account-balances-context';
import { formatErc20 } from '@/lib/decimals';

export type Token = SwapToken;

export function TokenIcon({ token, size = 28 }: { token: Token; size?: number }) {
  const colors: Record<string, string> = {
    USDC: '#2775CA',
    EURC: '#2B6CB0',
    CIRBTC: '#F7931A',
  };
  const bg = colors[token.symbol.toUpperCase()] ?? '#6B7280';
  return (
    <div
      className="rounded-full flex items-center justify-center text-white font-bold"
      style={{ width: size, height: size, backgroundColor: bg, fontSize: size * 0.4 }}
      aria-hidden
    >
      {token.symbol[0]}
    </div>
  );
}

export function TokenSelector({
  selected,
  onSelect,
  exclude,
  tokens: tokensOverride,
}: {
  selected: Token;
  onSelect: (token: Token) => void;
  exclude?: string;
  tokens?: Token[];
}) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const listRef = useRef<HTMLDivElement>(null);
  const { tokens: apiTokens } = useTokenCatalog();
  const tokens = tokensOverride ?? apiTokens;
  const { getErc20Balance, ready: balancesReady } = useAccountBalances();

  const qLower = search.trim().toLowerCase();
  const filtered = tokens.filter(
    (t) =>
      t.symbol.toLowerCase().includes(qLower) ||
      t.name.toLowerCase().includes(qLower) ||
      t.address.toLowerCase().includes(qLower),
  );
  const excludeLower = exclude?.toLowerCase();

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        setSearch('');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open]);

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 bg-[var(--surface-raised)] hover:bg-[var(--bg-0)] border border-[var(--border)] rounded-xl px-3.5 py-2.5 transition-colors"
      >
        <TokenIcon token={selected} size={24} />
        <span className="font-medium text-[15px] text-[var(--text-primary)]">
          {selected.symbol}
        </span>
        <svg
          className="w-3.5 h-3.5 text-[var(--text-muted)]"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          aria-hidden
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open &&
        typeof window !== 'undefined' &&
        createPortal(
          <div
            className="fixed inset-0 z-[200] flex items-center justify-center bg-black/75 backdrop-blur-[2px]"
            onClick={() => {
              setOpen(false);
              setSearch('');
            }}
          >
            <div
              className="bg-[var(--surface)] border border-[var(--border)] rounded-2xl w-full max-w-md mx-4 overflow-hidden"
              onClick={(e) => e.stopPropagation()}
            >
              <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--border)]">
                <h3 className="text-[15px] font-semibold text-[var(--text-primary)]">
                  Select a token
                </h3>
                <button
                  type="button"
                  onClick={() => {
                    setOpen(false);
                    setSearch('');
                  }}
                  className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                  aria-label="Close token selector"
                >
                  <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              </div>

              <div className="px-5 py-3">
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search token or address"
                  className="w-full bg-[var(--bg-0)] border border-[var(--border)] rounded-xl px-4 py-3 text-[13px] outline-none focus:border-[var(--accent)]/40 placeholder-[var(--text-muted)] text-[var(--text-primary)]"
                  autoFocus
                />
              </div>

              <div ref={listRef} className="max-h-[400px] overflow-y-auto px-2 pb-4">
                {filtered.length === 0 && (
                  <div className="px-4 py-6 text-center text-[var(--text-muted)] text-[13px]">
                    No tokens found
                  </div>
                )}
                {filtered.map((token) => {
                  const bal = balancesReady ? getErc20Balance(token.address) : null;
                  const isExcluded = excludeLower !== undefined && token.address.toLowerCase() === excludeLower;
                  return (
                    <button
                      key={token.address}
                      type="button"
                      onClick={() => {
                        if (!isExcluded) onSelect(token);
                        setOpen(false);
                        setSearch('');
                      }}
                      className={`w-full flex items-center gap-3 px-4 py-3 rounded-xl transition-colors ${
                        token.address === selected.address
                          ? 'bg-white/[0.03] border border-[var(--border)]'
                          : 'hover:bg-white/[0.03]'
                      }`}
                    >
                      <TokenIcon token={token} size={36} />
                      <div className="text-left min-w-0 flex-1">
                        <div className="text-sm font-semibold text-[var(--text-primary)]">
                          {token.symbol}
                        </div>
                        <div className="text-xs text-[var(--text-muted)] truncate">
                          {token.name}
                        </div>
                      </div>
                      {bal !== null && bal > BigInt(0) && (
                        <div className="text-xs text-[var(--text-secondary)] tabular-nums shrink-0 font-[family-name:var(--font-mono)]">
                          {formatErc20(bal, token.decimals)}
                        </div>
                      )}
                      {token.address === selected.address && (
                        <svg
                          className="w-4 h-4 text-[var(--accent)] shrink-0"
                          fill="currentColor"
                          viewBox="0 0 20 20"
                          aria-hidden
                        >
                          <path
                            fillRule="evenodd"
                            d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                            clipRule="evenodd"
                          />
                        </svg>
                      )}
                    </button>
                  );
                })}
                {filtered.length === 0 && tokenUnavailableNote(search) !== null && (
                  <div className="px-4 pb-2 text-center text-[11px] text-[var(--text-muted)]">
                    {tokenUnavailableNote(search)}
                  </div>
                )}
              </div>
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}

function tokenUnavailableNote(search: string): string | null {
  const q = search.trim().toLowerCase();
  if (q.includes('mbtc')) return 'mBTC is a local fixture — use cirBTC';
  if (q.includes('cirbtc')) return 'cirBTC is acquired via swap (e.g. USDC → EURC → cirBTC)';
  return null;
}
