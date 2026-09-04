import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import StatsPage from './page';

const BASE = 'http://stats.test';

type RouteHealth = {
  token_in: string;
  token_out: string;
  direct: boolean;
  multihop: boolean;
  usable_pools: number;
  best_sources: string[];
};

type Payload = {
  success: boolean;
  data: {
    meta: {
      chain: string;
      chain_head: number;
      confirmed_head: number;
      indexed_head: number;
      lag_blocks: number;
      freshness_secs: number | null;
      range: string;
      attributed_swaps: number;
      unattributed_swaps: number;
    };
    overview: {
      stablecoin_notional_micros: string;
      confirmed_swaps: number;
      unique_traders: number;
      split_swaps: number;
      split_share_bps: number;
    };
    daily: Array<{ day: string; stablecoin_notional_micros: string; swaps: number }>;
    venues: Array<{
      source: string;
      label: string;
      swap_participation: number;
      subroutes: number;
      hops: number;
      route_share_bps: number;
      pair_usage: string[];
    }>;
    route_health: RouteHealth[];
  };
};

function route(tokenIn: string, tokenOut: string, multihop: boolean): RouteHealth {
  return {
    token_in: tokenIn,
    token_out: tokenOut,
    direct: false,
    multihop,
    usable_pools: multihop ? 2 : 0,
    best_sources: multihop ? ['presto-hub'] : [],
  };
}

const CIRBTC = '0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF';
const USDC = '0x3600000000000000000000000000000000000000';
const EURC = '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a';

function payload(overrides?: Partial<Payload['data']>): Payload {
  const base: Payload['data'] = {
    meta: {
      chain: 'arc-testnet',
      chain_head: 60_001_000,
      confirmed_head: 60_000_988,
      indexed_head: 60_000_988,
      lag_blocks: 0,
      freshness_secs: 24,
      range: '30d',
      attributed_swaps: 1,
      unattributed_swaps: 0,
    },
    overview: {
      stablecoin_notional_micros: '1000000',
      confirmed_swaps: 1,
      unique_traders: 1,
      split_swaps: 0,
      split_share_bps: 0,
    },
    daily: [{ day: '2026-09-01', stablecoin_notional_micros: '1000000', swaps: 1 }],
    venues: [
      {
        source: 'presto-hub',
        label: 'presto-hub',
        swap_participation: 1,
        subroutes: 1,
        hops: 1,
        route_share_bps: 10000,
        pair_usage: ['USDC→EURC'],
      },
    ],
    route_health: [
      route(USDC, CIRBTC, true),
      route(CIRBTC, USDC, true),
      route(USDC, EURC, true),
      route(EURC, USDC, true),
      route(EURC, CIRBTC, true),
      route(CIRBTC, EURC, true),
    ],
  };
  return { success: true, data: { ...base, ...overrides } };
}

function jsonResponse(payload: Payload): Response {
  return {
    ok: true,
    status: 200,
    json: async () => payload,
  } as unknown as Response;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const DEFAULT_FETCH = globalThis.fetch;

beforeEach(() => {
  process.env.NEXT_PUBLIC_API_BASE_URL = BASE;
  process.env.NEXT_PUBLIC_CHAKRA_API_URL = '';
  window.history.replaceState(null, '', '/stats');
});

afterEach(() => {
  delete process.env.NEXT_PUBLIC_API_BASE_URL;
  delete process.env.NEXT_PUBLIC_CHAKRA_API_URL;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  globalThis.fetch = DEFAULT_FETCH;
});

describe('StatsPage', () => {
  it('shows content-shaped loading before data, then BigInt-formatted money', async () => {
    const pending = deferred<Response>();
    vi.stubGlobal(
      'fetch',
      vi.fn(() => pending.promise),
    );
    render(<StatsPage />);
    // Skeleton mirrors the content grid (status role while loading).
    expect(await screen.findByRole('status')).toBeTruthy();
    expect(screen.queryByText('$1.00')).toBeNull();
    pending.resolve(jsonResponse(payload()));
    // 1,000,000 micros renders $1.00 — never 1M.
    expect(await screen.findByText('$1.00')).toBeTruthy();
    expect(screen.queryByText('1M')).toBeNull();
  });

  it('surfaces a retryable error and recovers on retry', async () => {
    const calls = vi.fn();
    vi.stubGlobal(
      'fetch',
      vi.fn((...args: unknown[]) => {
        calls(args[0]);
        if (calls.mock.calls.length === 1) return Promise.reject(new Error('network down'));
        return Promise.resolve(jsonResponse(payload()));
      }),
    );
    render(<StatsPage />);
    expect(await screen.findByRole('alert')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByText('$1.00')).toBeTruthy();
    expect(calls.mock.calls.length).toBe(2);
  });

  it('renders honest empty states and zero values', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() =>
        Promise.resolve(
          jsonResponse(
            payload({
              daily: [],
              venues: [],
              route_health: [],
              overview: {
                stablecoin_notional_micros: '0',
                confirmed_swaps: 0,
                unique_traders: 0,
                split_swaps: 0,
                split_share_bps: 0,
              },
            }),
          ),
        ),
      ),
    );
    render(<StatsPage />);
    expect(await screen.findByText('$0.00')).toBeTruthy();
    expect(await screen.findByText('No confirmed swaps in this range yet.')).toBeTruthy();
    expect(screen.getByText('No venue participation in this range.')).toBeTruthy();
    expect(screen.getByText('Route probes have not reported yet.')).toBeTruthy();
  });

  it('reads the range from the URL and keeps selections in the URL', async () => {
    window.history.replaceState(null, '', '/stats?range=all');
    const calls: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn((url: unknown) => {
        calls.push(String(url));
        return Promise.resolve(jsonResponse(payload()));
      }),
    );
    render(<StatsPage />);
    await waitFor(() => expect(calls.length).toBe(1));
    expect(calls[0]).toContain('range=all');
    fireEvent.click(screen.getByRole('button', { name: 'Last 14 days' }));
    await waitFor(() => expect(calls.length).toBe(2));
    expect(calls[1]).toContain('range=14d');
    expect(window.location.search).toContain('range=14d');
    expect(screen.getByRole('button', { name: 'Last 14 days' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
  });

  it('prevents a stale response from replacing a newer range selection', async () => {
    const first = deferred<Response>();
    const second = deferred<Response>();
    vi.stubGlobal(
      'fetch',
      vi.fn((url: unknown) => (String(url).includes('range=30d') ? first.promise : second.promise)),
    );
    render(<StatsPage />);
    // Switch to 14d while the 30d request is still in flight.
    fireEvent.click(screen.getByRole('button', { name: 'Last 14 days' }));
    // Newer response lands first with $14.00…
    second.resolve(
      jsonResponse(
        payload({
          overview: { ...payload().data.overview, stablecoin_notional_micros: '14000000' },
        }),
      ),
    );
    expect(await screen.findByText('$14.00')).toBeTruthy();
    // …then the superseded 30d response arrives late with $30.00.
    first.resolve(
      jsonResponse(
        payload({
          overview: { ...payload().data.overview, stablecoin_notional_micros: '30000000' },
        }),
      ),
    );
    await waitFor(() => expect(first).toBeTruthy());
    // Give React a beat to flush any ignored state update.
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText('$30.00')).toBeNull();
    expect(screen.getByText('$14.00')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('shows canonical cirBTC naming and multihop status', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve(jsonResponse(payload()))),
    );
    render(<StatsPage />);
    await waitFor(() => expect(screen.getAllByText('cirBTC').length).toBeGreaterThan(0));
    // Token labels are used rather than raw hex prefixes.
    expect(screen.queryByText(/0xf0c4/i)).toBeNull();
    expect(screen.getAllByText('Multihop').length).toBeGreaterThanOrEqual(6);
  });
});
