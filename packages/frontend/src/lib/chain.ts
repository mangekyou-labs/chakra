/** Arc testnet chain gate (T6.1). */

import { arcTestnet } from 'wagmi/chains';

/** Arc testnet chain id 5042002 (0x4CEF52). */
export const ARC_CHAIN_ID = 5042002;
export const ARC_CHAIN_ID_HEX = '0x4CEF52';

/** Arc testnet public RPC + explorer — never Canteen `$RPC`, never invented URLs. */
export const ARC_RPC_URLS = ['https://rpc.testnet.arc.io'];
export const ARC_BLOCK_EXPLORER_URLS = ['https://testnet.arcscan.app'];

/**
 * `wallet_addEthereumChain` params. Matches the viem `arcTestnet` definition —
 * native gas is USDC at 18 dp (wallet may still label it ETH; UI copy says USDC).
 */
export const ARC_ADD_CHAIN_PARAMS = {
  chainId: ARC_CHAIN_ID_HEX,
  chainName: 'Arc Testnet',
  nativeCurrency: { name: 'USDC', symbol: 'USDC', decimals: 18 },
  rpcUrls: ARC_RPC_URLS,
  blockExplorerUrls: ARC_BLOCK_EXPLORER_URLS,
} as const;

/** True when the wallet's chain is Arc testnet (5042002). */
export function isArcTestnet(chainId: number | undefined): boolean {
  return chainId === arcTestnet.id;
}

/**
 * Native gas is USDC on Arc, regardless of what the wallet labels the native
 * asset (some wallets render ETH). On-screen copy always says USDC.
 */
export function nativeGasSymbol(_walletNativeSymbol?: string): string {
  return 'USDC';
}
