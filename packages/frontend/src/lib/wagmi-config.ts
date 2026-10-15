import { http } from 'wagmi';
import { createConfig, injected } from 'wagmi';
import { arc } from 'wagmi/chains';

/**
 * EIP-6963 injected connector only (T6.1). Arc mainnet official chain
 * definition — native gas is USDC 18 dp.
 */
export const wagmiConfig = createConfig({
  chains: [arc],
  connectors: [injected()],
  transports: {
    [arc.id]: http('https://rpc.mainnet.arc.io'),
  },
});

declare module 'wagmi' {
  interface Register {
    config: typeof wagmiConfig;
  }
}
