'use client';

import { createContext, useCallback, useContext, useEffect, useMemo, type ReactNode } from 'react';
import { useConnect, useConnection, useDisconnect, useSwitchChain } from 'wagmi';
import { AccountBalancesProvider } from '@/lib/account-balances-context';
import { ARC_CHAIN_ID, isArcMainnet } from '@/lib/chain';

export interface WalletState {
  address: `0x${string}` | null;
  chainId: number | undefined;
  /** True when the connected wallet is on Arc mainnet (5042). */
  onArcMainnet: boolean;
  connecting: boolean;
  connect: () => void;
  disconnect: () => void;
  /** `wallet_switchEthereumChain` → `wallet_addEthereumChain` fallback. */
  switchToArc: () => Promise<void>;
}

const WalletContext = createContext<WalletState>({
  address: null,
  chainId: undefined,
  onArcMainnet: false,
  connecting: false,
  connect: () => {},
  disconnect: () => {},
  switchToArc: async () => {},
});

export function useWallet() {
  return useContext(WalletContext);
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const { address, chainId, connector, isConnected, isConnecting } = useConnection();
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

  // MetaMask can switch without emitting chainChanged. Reconcile the selected
  // connector on restore and focus, updating Wagmi so all consumers agree.
  useEffect(() => {
    if (!isConnected || !connector) return;
    let active = true;
    const reconcile = async () => {
      try {
        const currentChainId = await connector.getChainId();
        if (active && currentChainId !== chainId) {
          connector.emitter.emit('change', { chainId: currentChainId });
        }
      } catch {
        // Keep the existing chain gate if the wallet is temporarily unavailable.
      }
    };
    const onVisible = () => {
      if (document.visibilityState === 'visible') void reconcile();
    };
    void reconcile();
    window.addEventListener('focus', reconcile);
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      active = false;
      window.removeEventListener('focus', reconcile);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [connector, chainId, isConnected]);

  const switchToArc = useCallback(async () => {
    // The connector handles unknown-chain (4902) fallback on its own provider.
    // User rejection must propagate rather than trigger a second wallet prompt.
    await switchChainAsync({ chainId: ARC_CHAIN_ID });
  }, [switchChainAsync]);

  const value = useMemo<WalletState>(
    () => ({
      address: isConnected ? (address ?? null) : null,
      chainId: isConnected ? chainId : undefined,
      onArcMainnet: isConnected ? isArcMainnet(chainId) : false,
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
