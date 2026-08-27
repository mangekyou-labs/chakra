'use client';

import { createContext, useCallback, useContext, useMemo, type ReactNode } from 'react';
import { useConnect, useConnection, useDisconnect, useSwitchChain } from 'wagmi';
import { AccountBalancesProvider } from '@/lib/account-balances-context';
import { ARC_ADD_CHAIN_PARAMS, isArcTestnet } from '@/lib/chain';

export interface WalletState {
  address: `0x${string}` | null;
  chainId: number | undefined;
  /** True when the connected wallet is on Arc testnet (5042002). */
  onArcTestnet: boolean;
  connecting: boolean;
  connect: () => void;
  disconnect: () => void;
  /** `wallet_switchEthereumChain` → `wallet_addEthereumChain` fallback. */
  switchToArc: () => Promise<void>;
}

const WalletContext = createContext<WalletState>({
  address: null,
  chainId: undefined,
  onArcTestnet: false,
  connecting: false,
  connect: () => {},
  disconnect: () => {},
  switchToArc: async () => {},
});

export function useWallet() {
  return useContext(WalletContext);
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const { address, chainId, isConnected, isConnecting } = useConnection();
  const { connect, connectors, isPending: connectPending } = useConnect();
  const { disconnect } = useDisconnect();
  const { switchChainAsync } = useSwitchChain();

  const connecting = connectPending || isConnecting;

  const connectWallet = useCallback(() => {
    // EIP-6963 injected connector; opens the wallet's own picker.
    const injectedConnector = connectors[0];
    if (injectedConnector) connect({ connector: injectedConnector });
  }, [connect, connectors]);

  const disconnectWallet = useCallback(() => {
    disconnect();
  }, [disconnect]);

  const switchToArc = useCallback(async () => {
    try {
      await switchChainAsync({ chainId: 5042002 });
    } catch {
      // Wallet does not know Arc — prompt `wallet_addEthereumChain`.
      const provider = (
        window as unknown as {
          ethereum?: {
            request?: (args: { method: string; params?: unknown[] }) => Promise<unknown>;
          };
        }
      ).ethereum;
      if (provider?.request) {
        await provider.request({
          method: 'wallet_addEthereumChain',
          params: [ARC_ADD_CHAIN_PARAMS],
        });
      }
    }
  }, [switchChainAsync]);

  const value = useMemo<WalletState>(
    () => ({
      address: isConnected ? (address ?? null) : null,
      chainId: isConnected ? chainId : undefined,
      onArcTestnet: isConnected ? isArcTestnet(chainId) : false,
      connecting,
      connect: connectWallet,
      disconnect: disconnectWallet,
      switchToArc,
    }),
    [address, chainId, isConnected, connecting, connectWallet, disconnectWallet, switchToArc],
  );

  return (
    <WalletContext.Provider value={value}>
      <AccountBalancesProvider>{children}</AccountBalancesProvider>
    </WalletContext.Provider>
  );
}
