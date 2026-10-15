/** Arc mainnet chain gate (T6.1). */

import { arc } from 'wagmi/chains';

/** Arc mainnet chain id 5042 (0x13B2). */
export const ARC_CHAIN_ID = 5042;
export const ARC_CHAIN_ID_HEX = '0x13B2';

/** Arc mainnet public RPC + explorer — never Canteen `$RPC`, never invented URLs. */
export const ARC_RPC_URLS = ['https://rpc.mainnet.arc.io'];
export const ARC_BLOCK_EXPLORER_URLS = ['https://explorer.arc.io'];

/**
 * `wallet_addEthereumChain` params. Matches the viem `arc` definition —
 * native gas is USDC at 18 dp (wallet may still label it ETH; UI copy says USDC).
 */
export const ARC_ADD_CHAIN_PARAMS = {
  chainId: ARC_CHAIN_ID_HEX,
  chainName: 'Arc Mainnet',
  nativeCurrency: { name: 'USDC', symbol: 'USDC', decimals: 18 },
  rpcUrls: ARC_RPC_URLS,
  blockExplorerUrls: ARC_BLOCK_EXPLORER_URLS,
} as const;

/** True when the wallet's chain is Arc mainnet (5042). */
export function isArcMainnet(chainId: number | undefined): boolean {
  return chainId === arc.id;
}

/**
 * Native gas is USDC on Arc, regardless of what the wallet labels the native
 * asset (some wallets render ETH). On-screen copy always says USDC.
 */
export function nativeGasSymbol(_walletNativeSymbol?: string): string {
  return 'USDC';
}
