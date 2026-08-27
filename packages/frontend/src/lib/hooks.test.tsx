import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { type ReactNode } from 'react';
import {
  useTokenCatalog,
  useBalanceQuery,
  useGasPriceQuery,
  useQuoteQuery,
  type QuoteQueryKey,
} from './hooks';
import { FALLBACK_SWAP_TOKENS } from './swap-tokens';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
      },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

beforeEach(() => {
  vi.restoreAllMocks();
});

describe('useTokenCatalog', () => {
  it('returns fallback when API is down', async () => {
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('network'));
    const { result } = renderHook(() => useTokenCatalog(), {
      wrapper: createWrapper(),
    });
    // Wait until isLoading goes false (retry: false means immediate failure)
    await waitFor(() => expect(result.current.isLoading).toBe(false), { timeout: 5000 });
    expect(result.current.tokens).toEqual(FALLBACK_SWAP_TOKENS);
  });

  it('returns tokens from successful API response', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      json: async () => ({
        success: true,
        data: {
          tokens: [
            { symbol: 'USDC', address: '0xabc', decimals: 6 },
            { symbol: 'EURC', address: '0xdef', decimals: 6 },
          ],
        },
      }),
    } as Response);
    const { result } = renderHook(() => useTokenCatalog(), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.tokens).toHaveLength(2);
    expect(result.current.tokens[0].symbol).toBe('USDC');
  });
});

describe('useBalanceQuery', () => {
  it('has undefined data when disconnected (query disabled)', () => {
    const { result } = renderHook(() => useBalanceQuery(null), {
      wrapper: createWrapper(),
    });
    // Query is disabled when address is null — data is undefined, not a result object
    expect(result.current.data).toBeUndefined();
    expect(result.current.fetchStatus).toBe('idle');
  });

  it('fetches balances when address provided', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      json: async () => ({
        success: true,
        data: { usdc: '1000000', eurc: '2000000', native_usdc: '500000' },
      }),
    } as Response);
    const { result } = renderHook(
      () => useBalanceQuery('0x1234567890abcdef1234567890abcdef12345678'),
      { wrapper: createWrapper() },
    );
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.balances).toBeDefined();
    expect(result.current.data?.nativeBalance).toBe(500000n);
  });
});

describe('useGasPriceQuery', () => {
  it('fetches gas price from Arc RPC', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      json: async () => ({ result: '0x3B9ACA00' }),
    } as Response);
    const { result } = renderHook(() => useGasPriceQuery(), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBe(1000000000n);
  });
});

describe('useQuoteQuery', () => {
  it('returns undefined data when key is null (disabled)', () => {
    const { result } = renderHook(() => useQuoteQuery(null), {
      wrapper: createWrapper(),
    });
    expect(result.current.data).toBeUndefined();
    expect(result.current.fetchStatus).toBe('idle');
  });

  it('fetches quote with valid key', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      json: async () => ({
        success: true,
        data: {
          amount_in: '1000000',
          expected_output: '2000000',
          minimum_output: '1900000',
          price_impact_bps: 50,
          protocol_fee_bps: 10,
          is_split: false,
          max_splits: 5,
          sub_routes: [],
          compute_time_ms: 10,
        },
      }),
    } as Response);

    const key: QuoteQueryKey = {
      tokenIn: '0xabc',
      tokenOut: '0xdef',
      amountIn: '1000000',
      slippageBps: 50,
    };

    const { result } = renderHook(() => useQuoteQuery(key), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.expected_output).toBe('2000000');
  });

  it('returns null when API returns no route', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      json: async () => ({
        success: false,
        error: { code: 'NO_ROUTE', message: 'No route' },
      }),
    } as Response);

    const key: QuoteQueryKey = {
      tokenIn: '0xabc',
      tokenOut: '0xdef',
      amountIn: '1000000',
      slippageBps: 50,
    };

    const { result } = renderHook(() => useQuoteQuery(key), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBeNull();
  });
});
