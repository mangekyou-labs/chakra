'use client';

import { useEffect } from 'react';
import { createPortal } from 'react-dom';
import { recordAck } from '@/lib/unaudited-ack';
type Props = {
  open: boolean;
  onAck: () => void;
};

/**
 * One-time unaudited-contract acknowledgement modal (T6.3).
 * Must be explicitly Acked to proceed with send.
 * Escape only after explicit dismiss is NOT required — user must Ack.
 */
export function UnauditedModal({ open, onAck }: Props) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open]);

  if (!open || typeof window === 'undefined') return null;

  const handleAck = () => {
    recordAck();
    onAck();
  };

  return createPortal(
    <div
      className="fixed inset-0 z-[210] flex items-center justify-center bg-black/75 backdrop-blur-[2px] p-4"
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="unaudited-title"
        className="w-full max-w-md rounded-2xl border border-[var(--border)] bg-[var(--surface)] shadow-xl"
      >
        <div className="px-5 py-5">
          <h3
            id="unaudited-title"
            className="text-[15px] font-semibold text-[var(--text-primary)] mb-3"
          >
            ⚠️ Unaudited Contracts
          </h3>
          <p className="text-[13px] text-[var(--text-secondary)] leading-relaxed mb-4">
            The smart contracts on Arc Testnet have <strong>not been audited</strong>. Use at your
            own risk. There is no guarantee of funds safety.
          </p>
          <p className="text-[12px] text-[var(--text-muted)] mb-5">
            This warning is shown once before your first swap.
          </p>
          <button
            type="button"
            onClick={handleAck}
            className="btn-primary w-full py-3 text-[15px]"
            autoFocus
          >
            I understand — proceed
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
