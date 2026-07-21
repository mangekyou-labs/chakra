'use client';

import { createContext, useContext, useState, useCallback, useEffect, type ReactNode } from 'react';
import { ArcWalletsKit } from '@creit.tech/Arc-wallets-kit';
import { KitEventType, Networks } from '@creit.tech/Arc-wallets-kit/types';
import { walletModule } from '@creit.tech/Arc-wallets-kit/modules/wallet';
import { xBullModule } from '@creit.tech/Arc-wallets-kit/modules/xbull';
import { LobstrModule } from '@creit.tech/Arc-wallets-kit/modules/lobstr';
import { AccountBalancesProvider } from '@/lib/account-balances-context';

export interface SignTxOptions {
  /** Defaults to Networks.PUBLIC (mainnet Instant). Pass Networks.TESTNET for Limit. */
  networkPassphrase?: string;
}

export interface WalletState {
  address: string | null;
  connecting: boolean;
  connect: () => void;
  disconnect: () => void;
  signTx: (xdr: string, opts?: SignTxOptions) => Promise<string>;
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
    modules: [new walletModule(), new xBullModule(), new LobstrModule()],
  });
  kitInitialized = true;
}

async function readStoredAddress(): Promise<string | null> {
  try {
    ensureKit();
    const { address } = await ArcWalletsKit.getAddress();
    return address || null;
  } catch {
    return null;
  }
}

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);

  // Restore kit session after full page loads (kit already persists address in localStorage).
  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const stored = await readStoredAddress();
      if (!cancelled && stored) setAddress(stored);
    })();

    ensureKit();
    const offState = ArcWalletsKit.on(KitEventType.STATE_UPDATED, (event) => {
      setAddress(event.payload.address ?? null);
    });
    const offDisconnect = ArcWalletsKit.on(KitEventType.DISCONNECT, () => {
      setAddress(null);
    });

    return () => {
      cancelled = true;
      offState();
      offDisconnect();
    };
  }, []);

  const connect = useCallback(async () => {
    setConnecting(true);
    try {
      ensureKit();
      const { address: addr } = await ArcWalletsKit.authModal();
      if (addr) {
        setAddress(addr);
      }
    } catch (err: unknown) {
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

  const signTx = useCallback(
    async (xdr: string, opts?: SignTxOptions): Promise<string> => {
      ensureKit();
      const { signedTxXdr } = await ArcWalletsKit.signTransaction(xdr, {
        networkPassphrase: opts?.networkPassphrase ?? Networks.PUBLIC,
        address: address ?? undefined,
      });
      return signedTxXdr;
    },
    [address],
  );

  return (
    <WalletContext.Provider value={{ address, connecting, connect, disconnect, signTx }}>
      <AccountBalancesProvider>{children}</AccountBalancesProvider>
    </WalletContext.Provider>
  );
}
