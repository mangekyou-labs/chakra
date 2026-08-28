/**
 * qa/wallet/constants.ts
 *
 * Production-aligned constants for the T9.4 MetaMask critical path (T9.4 prep).
 * These mirror `packages/frontend/src/lib/chain.ts` and
 * `packages/frontend/src/lib/recent-swaps.ts` — never hard-code a divergent
 * URL or storage key in the spec.
 */

/** Arc testnet chain id 5042002 (0x4CEF52). */
export const QA_CHAIN_ID = 5042002;

/** Public Arc RPC — never Canteen `$RPC`, never invented Alchemy URLs. */
export const QA_RPC_URL = 'https://rpc.testnet.arc.io';

/** Explorer matches `ARC_BLOCK_EXPLORER_URLS` in src/lib/chain.ts (.app). */
export const QA_EXPLORER_URL = 'https://testnet.arcscan.app';

/** ERC-20 USDC (6 dp) and EURC (6 dp) catalog addresses. */
export const QA_USDC = '0x3600000000000000000000000000000000000000';
export const QA_EURC = '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a';

/** Recent-swaps storage key prefix — matches src/lib/recent-swaps.ts. */
export const QA_STORAGE_PREFIX = 'chakra:recent-swaps';

/** Success banner copy — matches SwapCard.tsx confirmed state. */
export const QA_SWAP_CONFIRMED_TEXT = 'Swap confirmed!';
