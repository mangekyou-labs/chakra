import { describe, it, expect, beforeEach, vi } from 'vitest';
import { addRecentSwap, getRecentSwaps, RecentSwap, RECENT_SWAPS_MAX } from './recent-swaps';

// Minimal localStorage mock for node vitest
const store = new Map<string, string>();
const mockStorage = {
  getItem: vi.fn((k: string) => store.get(k) ?? null),
  setItem: vi.fn((k: string, v: string) => {
    store.set(k, v);
  }),
  removeItem: vi.fn((k: string) => {
    store.delete(k);
  }),
  clear: vi.fn(() => {
    store.clear();
  }),
  get length() {
    return store.size;
  },
  key: vi.fn((i: number) => [...store.keys()][i] ?? null),
};

// Inject into globalThis before each test
vi.stubGlobal('localStorage', mockStorage);

const ADDR = '0x1234567890abcdef1234567890abcdef12345678';
const CHAIN_ID = 5042002;

describe('recent-swaps', () => {
  beforeEach(() => {
    store.clear();
    vi.clearAllMocks();
  });

  it('returns empty array when no swaps stored', () => {
    const swaps = getRecentSwaps(CHAIN_ID, ADDR);
    expect(swaps).toEqual([]);
  });

  it('stores and retrieves a swap', () => {
    const swap: Omit<RecentSwap, 'timestamp'> = {
      txHash: '0xabc123',
      tokenIn: 'USDC',
      tokenOut: 'EURC',
      amountIn: '1000000',
      amountOut: '990000',
      isSplit: false,
    };
    addRecentSwap(CHAIN_ID, ADDR, swap);
    const swaps = getRecentSwaps(CHAIN_ID, ADDR);
    expect(swaps).toHaveLength(1);
    expect(swaps[0].txHash).toBe('0xabc123');
    expect(swaps[0].tokenIn).toBe('USDC');
    expect(swaps[0].tokenOut).toBe('EURC');
    expect(swaps[0].amountIn).toBe('1000000');
    expect(swaps[0].amountOut).toBe('990000');
    expect(swaps[0].isSplit).toBe(false);
    expect(swaps[0].timestamp).toBeGreaterThan(0);
  });

  it('stores swaps newest first', () => {
    addRecentSwap(CHAIN_ID, ADDR, {
      txHash: '0x111',
      tokenIn: 'USDC',
      tokenOut: 'EURC',
      amountIn: '1000',
      amountOut: '990',
      isSplit: false,
    });
    addRecentSwap(CHAIN_ID, ADDR, {
      txHash: '0x222',
      tokenIn: 'EURC',
      tokenOut: 'cirBTC',
      amountIn: '5000',
      amountOut: '100',
      isSplit: true,
    });
    const swaps = getRecentSwaps(CHAIN_ID, ADDR);
    expect(swaps).toHaveLength(2);
    expect(swaps[0].txHash).toBe('0x222');
    expect(swaps[1].txHash).toBe('0x111');
  });

  it('drops oldest beyond max (20)', () => {
    for (let i = 0; i < RECENT_SWAPS_MAX + 5; i++) {
      addRecentSwap(CHAIN_ID, ADDR, {
        txHash: `0x${i.toString(16).padStart(3, '0')}`,
        tokenIn: 'USDC',
        tokenOut: 'EURC',
        amountIn: String(i),
        amountOut: String(i),
        isSplit: false,
      });
    }
    const swaps = getRecentSwaps(CHAIN_ID, ADDR);
    expect(swaps).toHaveLength(RECENT_SWAPS_MAX);
    expect(swaps.find((s) => s.txHash === '0x000')).toBeUndefined();
  });

  it('is address case-insensitive', () => {
    addRecentSwap(CHAIN_ID, '0xABC', {
      txHash: '0xaaa',
      tokenIn: 'USDC',
      tokenOut: 'EURC',
      amountIn: '1',
      amountOut: '1',
      isSplit: false,
    });
    const swaps = getRecentSwaps(CHAIN_ID, '0xabc');
    expect(swaps).toHaveLength(1);
  });

  it('differentiates by chain id', () => {
    addRecentSwap(CHAIN_ID, ADDR, {
      txHash: '0xaaa',
      tokenIn: 'USDC',
      tokenOut: 'EURC',
      amountIn: '1',
      amountOut: '1',
      isSplit: false,
    });
    addRecentSwap(1, ADDR, {
      txHash: '0xbbb',
      tokenIn: 'ETH',
      tokenOut: 'USDC',
      amountIn: '2',
      amountOut: '2',
      isSplit: false,
    });
    expect(getRecentSwaps(CHAIN_ID, ADDR)).toHaveLength(1);
    expect(getRecentSwaps(1, ADDR)).toHaveLength(1);
  });

  it('explorer URL is derived (not stored)', () => {
    addRecentSwap(CHAIN_ID, ADDR, {
      txHash: '0xdeadbeef',
      tokenIn: 'USDC',
      tokenOut: 'EURC',
      amountIn: '100',
      amountOut: '99',
      isSplit: false,
    });
    const swaps = getRecentSwaps(CHAIN_ID, ADDR);
    expect('explorerUrl' in swaps[0]).toBe(false);
  });
});
