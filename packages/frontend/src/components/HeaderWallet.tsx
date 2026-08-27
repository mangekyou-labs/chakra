'use client';

import { useEffect, useRef, useState } from 'react';
import { useWallet } from '@/lib/wallet-context';
import { nativeGasSymbol } from '@/lib/chain';

export function HeaderWallet() {
  const { address, onArcTestnet, connect, disconnect, switchToArc, connecting } = useWallet();
  const [showMenu, setShowMenu] = useState(false);
  const [switching, setSwitching] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!showMenu) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) {
        setShowMenu(false);
      }
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  }, [showMenu]);

  const handleSwitch = async () => {
    setSwitching(true);
    try {
      await switchToArc();
    } finally {
      setSwitching(false);
    }
  };

  if (address) {
    return (
      <div className="relative z-50" ref={menuRef}>
        <button
          onClick={() => setShowMenu(!showMenu)}
          className="flex items-center gap-2 rounded-full bg-[var(--surface-raised)] px-4 py-2 transition-colors hover:bg-[var(--surface)]"
        >
          <span
            className={`w-1.5 h-1.5 rounded-full ${onArcTestnet ? 'bg-[var(--accent)]' : 'bg-amber-400'}`}
          />
          <span className="text-[14px] font-[family-name:var(--font-mono)] text-[var(--text-primary)]">
            {address.slice(0, 4)}…{address.slice(-4)}
          </span>
        </button>

        {showMenu && (
          <div className="absolute right-0 top-full mt-2 z-[60] bg-[var(--surface)] border border-[var(--border)] rounded-xl overflow-hidden min-w-[220px]">
            {!onArcTestnet && (
              <button
                onClick={() => {
                  void handleSwitch();
                  setShowMenu(false);
                }}
                disabled={switching}
                className="w-full px-4 py-2.5 text-left text-[13px] text-amber-300 hover:bg-amber-500/[0.06] disabled:opacity-50 transition-colors"
              >
                {switching ? 'Switching…' : 'Switch to Arc Testnet'}
              </button>
            )}
            <div className="px-4 py-3 text-[11px] text-[var(--text-muted)] font-[family-name:var(--font-mono)] break-all border-b border-[var(--border)]">
              {address}
              <span className="block mt-1 text-[10px] text-[var(--text-muted)]">
                Gas: {nativeGasSymbol()}
              </span>
            </div>
            <button
              onClick={() => {
                disconnect();
                setShowMenu(false);
              }}
              className="w-full px-4 py-2.5 text-left text-[13px] text-red-400/90 hover:bg-red-500/[0.06] transition-colors"
            >
              Disconnect
            </button>
          </div>
        )}
      </div>
    );
  }

  return (
    <button
      onClick={connect}
      disabled={connecting}
      className="rounded-full bg-[var(--accent)] px-5 py-2.5 text-[15px] font-semibold text-[var(--accent-contrast)] hover:bg-[var(--accent-hover)] disabled:opacity-50 transition-colors"
    >
      {connecting ? 'Connecting…' : 'Connect Wallet'}
    </button>
  );
}
