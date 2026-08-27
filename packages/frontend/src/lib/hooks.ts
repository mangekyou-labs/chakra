/**
 * TanStack Query hooks for Chakra API reads.
 */
import { useQuery } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { CHAKRA_API_URL } from '@/lib/aggregator';
import { FALLBACK_SWAP_TOKENS, filterSwapTokens } from '@/lib/swap-tokens';
import { USDC_ERC20_ADDRESS, EURC_ADDRESS, NATIVE_USDC_KEY } from '@/lib/decimals';

const ARC_RPC_URL = 'https://rpc.testnet.arc.io';

// ── Token Catalog ──────────────────────────────────────────────────────

export interface CatalogToken {
  address: string;
  symbol: string;
  name: string;
  decimals: number;
}

export function useTokenCatalog(): {
  tokens: CatalogToken[];
  isLoading: boolean;
} {
  const { data, isLoading } = useQuery({
    queryKey: ['chakra', 'tokens'],
    queryFn: async (): Promise<CatalogToken[]> => {
      const resp = await fetch(`${CHAKRA_API_URL}/api/v1/tokens`);
      const json = (await resp.json()) as {
        success?: boolean;
        data?: { tokens?: Array<{ symbol: string; address: string; decimals: number }> };
      };
      if (!json.success || !json.data?.tokens) return FALLBACK_SWAP_TOKENS;
      const rows = json.data.tokens;
      const list = filterSwapTokens(rows);
      if (list.length === 0) return FALLBACK_SWAP_TOKENS;
      return list.map((t) => ({
        address: t.address.toLowerCase(),
        symbol: t.symbol,
        name: t.name,
        decimals: t.decimals,
      }));
    },
    staleTime: 5 * 60 * 1000,
    retry: 1,
    refetchOnWindowFocus: false,
  });

  return {
    tokens: data ?? FALLBACK_SWAP_TOKENS,
    isLoading,
  };
}

// ── Balances ───────────────────────────────────────────────────────────

export interface BalanceData {
  balances: Record<string, bigint>;
  nativeBalance: bigint | null;
}

export function useBalanceQuery(address: string | null) {
  const normalizedAddress = address?.toLowerCase() ?? '';
  return useQuery({
    queryKey: ['chakra', 'balances', normalizedAddress],
    queryFn: async ({ signal }): Promise<BalanceData> => {
      if (!normalizedAddress) throw new Error('no address');
      const resp = await fetch(`${CHAKRA_API_URL}/api/v1/balances?account=${normalizedAddress}`, {
        signal,
      });
      const json = (await resp.json()) as {
        success?: boolean;
        data?: Record<string, string>;
      };
      if (!json.success || !json.data) {
        return { balances: {}, nativeBalance: null };
      }
      const next: Record<string, bigint> = {};
      let nativeBalance: bigint | null = null;
      for (const [key, raw] of Object.entries(json.data)) {
        if (key === NATIVE_USDC_KEY) {
          nativeBalance = BigInt(raw || '0');
        } else if (key === 'usdc') {
          next[USDC_ERC20_ADDRESS] = BigInt(raw || '0');
        } else if (key === 'eurc') {
          next[EURC_ADDRESS] = BigInt(raw || '0');
        } else {
          next[key] = BigInt(raw || '0');
        }
      }
      return { balances: next, nativeBalance };
    },
    enabled: !!normalizedAddress,
    staleTime: 10_000,
    refetchOnWindowFocus: false,
  });
}

// ── Gas Price ──────────────────────────────────────────────────────────

export function useGasPriceQuery() {
  return useQuery({
    queryKey: ['arc', 'gas-price'],
    queryFn: async (): Promise<bigint> => {
      const res = await fetch(ARC_RPC_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'eth_gasPrice', params: [] }),
      });
      const json = (await res.json()) as { result?: string };
      if (!json.result) throw new Error('no gas price result');
      return BigInt(json.result);
    },
    refetchInterval: 30_000,
    staleTime: 25_000,
    refetchOnWindowFocus: false,
    placeholderData: (prev) => prev,
  });
}

// ── Quote (debounced + polling) ────────────────────────────────────────

export interface QuoteQueryKey {
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  slippageBps: number;
}

export function useQuoteQuery(key: QuoteQueryKey | null, debounceMs = 250) {
  const [debouncedKey, setDebouncedKey] = useState<QuoteQueryKey | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Debounce using effect (no cascading setState)
  useEffect(() => {
    if (timeoutRef.current) clearTimeout(timeoutRef.current);
    if (!key || !key.amountIn || key.amountIn === '0') {
      setDebouncedKey(null);
      return;
    }
    timeoutRef.current = setTimeout(() => setDebouncedKey(key), debounceMs);
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, [key, debounceMs]);

  const isStale =
    debouncedKey !== null &&
    key !== null &&
    (debouncedKey.tokenIn !== key.tokenIn ||
      debouncedKey.tokenOut !== key.tokenOut ||
      debouncedKey.amountIn !== key.amountIn ||
      debouncedKey.slippageBps !== key.slippageBps);

  const query = useQuery({
    queryKey: debouncedKey
      ? [
          'chakra',
          'quote',
          debouncedKey.tokenIn,
          debouncedKey.tokenOut,
          debouncedKey.amountIn,
          debouncedKey.slippageBps,
        ]
      : ['chakra', 'quote', 'disabled'],
    queryFn: async ({ signal }) => {
      if (!debouncedKey) throw new Error('no key');
      const params = new URLSearchParams({
        token_in: debouncedKey.tokenIn,
        token_out: debouncedKey.tokenOut,
        amount_in: debouncedKey.amountIn,
        slippage_bps: String(debouncedKey.slippageBps),
      });
      const resp = await fetch(`${CHAKRA_API_URL}/api/v1/quote?${params}`, { signal });
      const json = (await resp.json()) as {
        success?: boolean;
        data?: import('@/lib/aggregator').QuoteData;
        error?: { code: string; message: string };
      };
      if (!json.success || !json.data) {
        return null;
      }
      return json.data;
    },
    enabled: !!debouncedKey,
    refetchInterval: debouncedKey ? 5000 : false,
    staleTime: 4000,
    refetchOnWindowFocus: false,
    retry: false,
  });

  return {
    ...query,
    isStale,
  };
}
