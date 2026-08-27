import { describe, it, expect, beforeEach, vi } from 'vitest';
import { hasAck, recordAck, UNAUDITED_ACK_KEY } from './unaudited-ack';

// Minimal localStorage mock
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
vi.stubGlobal('localStorage', mockStorage);

describe('unaudited-ack', () => {
  beforeEach(() => {
    store.clear();
    vi.clearAllMocks();
  });

  it('storage key is chakra:unaudited-ack:v1', () => {
    expect(UNAUDITED_ACK_KEY).toBe('chakra:unaudited-ack:v1');
  });

  it('hasAck returns false when key is missing', () => {
    expect(hasAck()).toBe(false);
  });

  it('hasAck returns true after recordAck', () => {
    recordAck();
    expect(hasAck()).toBe(true);
  });

  it('recordAck stores an ISO timestamp', () => {
    recordAck();
    const raw = store.get(UNAUDITED_ACK_KEY);
    expect(raw).toBeDefined();
    // Should be valid ISO date
    expect(Number.isFinite(Date.parse(raw!))).toBe(true);
  });

  it('hasAck returns false on corrupted localStorage value', () => {
    store.set(UNAUDITED_ACK_KEY, '{corrupt');
    expect(hasAck()).toBe(false);
  });

  it('handles missing localStorage gracefully', () => {
    // Temporarily replace with null storage
    void vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('no storage');
      },
      setItem: () => {
        throw new Error('no storage');
      },
    });
    expect(hasAck()).toBe(false);
    // Should not throw
    recordAck();
    vi.stubGlobal('localStorage', mockStorage);
  });
});
