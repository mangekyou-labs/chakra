import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import { WalletProvider, useWallet } from './wallet-context';

const mock = vi.hoisted(() => ({
  chainId: 5042002,
  actualChainId: 5042,
  emit: vi.fn(),
  switchChainAsync: vi.fn(),
  request: vi.fn(),
}));
const connector = {
  getChainId: async () => mock.actualChainId,
  getProvider: async () => ({ request: mock.request }),
  emitter: { emit: mock.emit },
};
vi.mock('wagmi', () => ({
  useConnection: () => ({
    address: '0x0000000000000000000000000000000000000001',
    chainId: mock.chainId,
    isConnected: true,
    isConnecting: false,
    connector,
  }),
  useConnect: () => ({ connect: vi.fn(), connectors: [], isPending: false }),
  useDisconnect: () => ({ disconnect: vi.fn() }),
  useSwitchChain: () => ({ switchChainAsync: mock.switchChainAsync }),
}));
vi.mock('./account-balances-context', () => ({
  AccountBalancesProvider: ({ children }: { children: ReactNode }) => children,
}));
const wrapper = ({ children }: { children: ReactNode }) => (
  <WalletProvider>{children}</WalletProvider>
);
beforeEach(() => {
  vi.clearAllMocks();
  mock.chainId = 5042002;
  mock.actualChainId = 5042;
  mock.switchChainAsync.mockResolvedValue(undefined);
});
describe('wallet network reconciliation', () => {
  it('refreshes a restored testnet connection from the selected mainnet connector', async () => {
    const { result, rerender } = renderHook(useWallet, { wrapper });
    await waitFor(() => expect(mock.emit).toHaveBeenCalledWith('change', { chainId: 5042 }));
    mock.chainId = 5042;
    rerender();
    expect(result.current.onArcMainnet).toBe(true);
  });
  it('rechecks the selected connector when returning from the wallet panel', async () => {
    mock.chainId = 5042;
    const { result, rerender } = renderHook(useWallet, { wrapper });
    expect(result.current.onArcMainnet).toBe(true);
    mock.actualChainId = 1;
    act(() => window.dispatchEvent(new Event('focus')));
    await waitFor(() => expect(mock.emit).toHaveBeenCalledWith('change', { chainId: 1 }));
    mock.chainId = 1;
    rerender();
    expect(result.current.onArcMainnet).toBe(false);
  });
  it('does not add a chain after the user rejects switching', async () => {
    mock.switchChainAsync.mockRejectedValue({ code: 4001 });
    const { result } = renderHook(useWallet, { wrapper });
    await expect(result.current.switchToArc()).rejects.toMatchObject({ code: 4001 });
    expect(mock.request).not.toHaveBeenCalled();
  });
});
