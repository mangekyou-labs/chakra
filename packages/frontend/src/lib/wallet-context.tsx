'use client';

import { createContext, useContext, useState, useCallback, type ReactNode } from 'react';
import { ArcWalletsKit } from '@creit.tech/Arc-wallets-kit';
import { Networks } from '@creit.tech/Arc-wallets-kit/types';
import { walletModule } from '@creit.tech/Arc-wallets-kit/modules/wallet';
import { xBullModule } from '@creit.tech/Arc-wallets-kit/modules/xbull';
import { LobstrModule } from '@creit.tech/Arc-wallets-kit/modules/lobstr';
import { AccountBalancesProvider } from '@/lib/account-balances-context';

export interface WalletState {
  address: string | null;
  connecting: boolean;
  connect: () => void;
  disconnect: () => void;
  signTx: (xdr: string) => Promise<string>;
}

const WalletContext = createContext<WalletState>({
  address: null,
  connecting: false,
  connect: () => {},
  disconnect: () => {},
  signTx: async () => '',
});

export function useWallet() {
  return useContext(WalletContext);
}

let kitInitialized = false;

function ensureKit() {
  if (kitInitialized) return;
  ArcWalletsKit.init({
    // Must be the full passphrase, not the shorthand "public" (wallet rejects signing otherwise).
    network: Networks.PUBLIC,
    modules: [
      new walletModule(),
      new xBullModule(),
      new LobstrModule(),
    ],
  });
  kitInitialized = true;
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);

  const connect = useCallback(async () => {
    setConnecting(true);
    try {
      ensureKit();
      const { address: addr } = await ArcWalletsKit.authModal();
      if (addr) {
        setAddress(addr);
      }
    } catch (err: any) {
      console.error('Wallet connect error:', err);
    } finally {
      setConnecting(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    try {
      await ArcWalletsKit.disconnect();
    } catch {}
    setAddress(null);
  }, []);

  const signTx = useCallback(async (xdr: string): Promise<string> => {
    ensureKit();
    const { signedTxXdr } = await ArcWalletsKit.signTransaction(xdr, {
      networkPassphrase: Networks.PUBLIC,
      address: address ?? undefined,
    });
    return signedTxXdr;
  }, [address]);

  return (
    <WalletContext.Provider value={{ address, connecting, connect, disconnect, signTx }}>
      <AccountBalancesProvider>{children}</AccountBalancesProvider>
    </WalletContext.Provider>
  );
}
