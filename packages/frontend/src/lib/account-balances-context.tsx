'use client';

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { useWallet } from '@/lib/wallet-context';
import { CHAKRA_API_URL } from '@/lib/aggregator';
import {
  CIRBTC_ADDRESS,
  USDC_ERC20_ADDRESS,
  EURC_ADDRESS,
  NATIVE_USDC_KEY,
} from '@/lib/decimals';

export interface AccountBalancesState {
  /** ERC-20 balances keyed by lowercase token address (never native). */
  balances: Record<string, bigint>;
  /** Native USDC (18 dp) gas — separate field, never summed. */
  nativeBalance: bigint | null;
  loading: boolean;
  ready: boolean;
  refresh: () => Promise<void>;
  getErc20Balance: (tokenAddress: string) => bigint | null;
}

const AccountBalancesContext = createContext<AccountBalancesState>({
  balances: {},
  nativeBalance: null,
  loading: false,
  ready: false,
  refresh: async () => {},
  getErc20Balance: () => null,
});

export function useAccountBalances() {
  return useContext(AccountBalancesContext);
}

export function AccountBalancesProvider({ children }: { children: ReactNode }) {
  const { address } = useWallet();
  const [balances, setBalances] = useState<Record<string, bigint>>({});
  const [nativeBalance, setNativeBalance] = useState<bigint | null>(null);
  const [loading, setLoading] = useState(false);
  const [ready, setReady] = useState(false);
  const requestId = useRef(0);

  const refresh = useCallback(async () => {
    const id = ++requestId.current;
    if (!address) {
      setBalances({});
      setNativeBalance(null);
      setReady(false);
      return;
    }

    setLoading(true);
    try {
      const resp = await fetch(`${CHAKRA_API_URL}/api/v1/balances?account=${address}`);
      const json = (await resp.json()) as {
        success?: boolean;
        data?: Record<string, string>;
        error?: { code?: string; message?: string };
      };
      if (id !== requestId.current) return;
      if (!json.success || !json.data) {
        setBalances({});
        setNativeBalance(null);
        setReady(false);
        return;
      }
      const next: Record<string, bigint> = {};
      for (const [key, raw] of Object.entries(json.data)) {
        if (key === NATIVE_USDC_KEY) continue;
        if (key === 'usdc') next[USDC_ERC20_ADDRESS] = BigInt(raw || '0');
        else if (key === 'eurc') next[EURC_ADDRESS] = BigInt(raw || '0');
        else if (key === 'cirbtc') next[CIRBTC_ADDRESS] = BigInt(raw || '0');
        else next[key] = BigInt(raw || '0');
      }
      setBalances(next);
      setNativeBalance(BigInt(json.data[NATIVE_USDC_KEY] ?? '0'));
      setReady(true);
    } catch {
      if (id === requestId.current) {
        setBalances({});
        setNativeBalance(null);
        setReady(false);
      }
    } finally {
      if (id === requestId.current) setLoading(false);
    }
  }, [address]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const getErc20Balance = useCallback(
    (tokenAddress: string) => {
      const key = tokenAddress.toLowerCase();
      if (balances[key] !== undefined) return balances[key];
      if (!ready) return null;
      return null;
    },
    [balances, ready],
  );

  return (
    <AccountBalancesContext.Provider
      value={{ balances, nativeBalance, loading, ready, refresh, getErc20Balance }}
    >
      {children}
    </AccountBalancesContext.Provider>
  );
}
