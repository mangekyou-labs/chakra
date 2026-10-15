/**
 * qa/wallet/constants.ts
 *
 * Production-aligned constants for the T9.4 MetaMask critical path (T9.4 prep).
 * These mirror `packages/frontend/src/lib/chain.ts` and
 * `packages/frontend/src/lib/recent-swaps.ts` — never hard-code a divergent
 * URL or storage key in the spec.
 */

/** Arc mainnet chain id 5042 (0x13B2). */
export const QA_CHAIN_ID = 5042;

/** Public Arc RPC — never Canteen `$RPC`, never invented Alchemy URLs. */
export const QA_RPC_URL = 'https://rpc.mainnet.arc.io';

/** Explorer matches `ARC_BLOCK_EXPLORER_URLS` in src/lib/chain.ts (.app). */
export const QA_EXPLORER_URL = 'https://explorer.arc.io';

/** ERC-20 USDC (6 dp) and EURC (6 dp) catalog addresses. */
export const QA_USDC = '0x3600000000000000000000000000000000000000';
export const QA_EURC = '0xbEf5f6d51CB62b58e6A8f77868681825C6fe21c1';

/** Recent-swaps storage key prefix — matches src/lib/recent-swaps.ts. */
export const QA_STORAGE_PREFIX = 'chakra:recent-swaps';

/** Success banner copy — matches SwapCard.tsx confirmed state. */
export const QA_SWAP_CONFIRMED_TEXT = 'Swap confirmed!';
