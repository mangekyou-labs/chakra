'use client';

import { useEffect, useState } from 'react';
import {
  getSubmitViaPreference,
  setSubmitViaPreference,
  type SubmitNetwork,
  type SubmitVia,
} from '@/lib/rpc';

export function SubmitViaToggle({ network = 'public' }: { network?: SubmitNetwork }) {
  const [submitVia, setSubmitVia] = useState<SubmitVia>('Chakra');

  useEffect(() => {
    setSubmitVia(getSubmitViaPreference());
  }, []);

  const host =
    network === 'testnet' ? 'Arc-testnet.Arc.org' : 'mainnet.Arcrpc.com';

  return (
    <label className="mt-4 flex items-start gap-2 cursor-pointer select-none text-[11px] leading-snug text-[var(--text-muted)]/70 hover:text-[var(--text-muted)]">
      <input
        type="checkbox"
        className="mt-0.5 accent-[var(--accent)]"
        checked={submitVia === 'official'}
        onChange={(e) => {
          const next: SubmitVia = e.target.checked ? 'official' : 'Chakra';
          setSubmitVia(next);
          setSubmitViaPreference(next);
        }}
      />
      <span>
        Advanced: submit via official RPC
        {submitVia === 'official' ? ` (${host})` : ''}
      </span>
    </label>
  );
}
