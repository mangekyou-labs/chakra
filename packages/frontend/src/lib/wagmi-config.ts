import { http } from 'wagmi';
import { createConfig, injected } from 'wagmi';
import { arcTestnet } from 'wagmi/chains';

/**
 * EIP-6963 injected connector only (T6.1). Arc testnet official chain
 * definition — native gas is USDC 18 dp.
 */
export const wagmiConfig = createConfig({
  chains: [arcTestnet],
  connectors: [injected()],
  transports: {
    [arcTestnet.id]: http('https://rpc.testnet.arc.io'),
  },
});

declare module 'wagmi' {
  interface Register {
    config: typeof wagmiConfig;
  }
}
