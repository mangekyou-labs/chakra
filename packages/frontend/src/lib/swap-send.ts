/**
 * Swap send helpers (T6.3).
 *
 * Gas estimation, chain gate, paused check, and Permit2 signature splice.
 * Public Arc RPC only: `https://rpc.testnet.arc.io`.
 */

import type { BuildTxData } from './aggregator';

/** Arc testnet chain id. */
export const ARC_CHAIN_ID = 5042002;

/**
 * Minimum 20 gwei `maxFeePerGas` — ensures competitive inclusion on Arc.
 * `max(suggested, 20 gwei)`.
 */
export const MIN_FEE_PER_GAS_WEI = BigInt(20) * BigInt(1e9);

/** Public Arc testnet RPC — never Canteen `$RPC`, never invented URLs. */
const ARC_RPC_URL = 'https://rpc.testnet.arc.io';

/** splitSwap function selector (8 hex chars). */
const SPLIT_SWAP_SELECTOR = '2e3be0c1';

/** ERC-20 approve(Permit2, amount) selector. */
const ERC20_APPROVE_SELECTOR = '095ea7b3';

/** Permit2 `permit(PermitSingle,bytes)` selector (AllowanceTransfer). */
const PERMIT2_PERMIT_SELECTOR = '97498857';

/**
 * `max(suggested, 20 gwei)`.
 */
export function minFeePerGas(suggested: bigint): bigint {
  return suggested > MIN_FEE_PER_GAS_WEI ? suggested : MIN_FEE_PER_GAS_WEI;
}

/** Fetch `eth_feeHistory` suggested gas price, falling back to `eth_gasPrice`. */
export async function fetchSuggestedFee(): Promise<bigint> {
  try {
    const feeHistoryRes = await fetch(ARC_RPC_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'eth_feeHistory',
        params: ['0x1', 'latest', [75]],
      }),
    });
    const feeHistoryJson = (await feeHistoryRes.json()) as {
      result?: { baseFeePerGas?: string[]; reward?: string[][] };
    };
    // T6.3: maxFeePerGas must be base + priority tip. The priority reward
    // alone would underpay the base fee.
    const baseFee = feeHistoryJson.result?.baseFeePerGas?.[1];
    const tip = feeHistoryJson.result?.reward?.[0]?.[0];
    if (baseFee) {
      const base = BigInt(baseFee);
      const priority = tip ? BigInt(tip) : 0n;
      return base + priority;
    }
    if (tip) return BigInt(tip);
  } catch {
    // Fall through
  }

  try {
    const gasPriceRes = await fetch(ARC_RPC_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'eth_gasPrice', params: [] }),
    });
    const gasPriceJson = (await gasPriceRes.json()) as { result?: string };
    if (gasPriceJson.result) return BigInt(gasPriceJson.result);
  } catch {
    // RPC completely down
  }

  return MIN_FEE_PER_GAS_WEI;
}

/**
 * Build wallet send parameters from the `build_tx` envelope.
 * `value` is always `"0"` (SC-12); `maxFeePerGas` ≥ 20 gwei.
 */
export function buildSendParams(
  tx: BuildTxData,
  suggestedFee: bigint,
): {
  to: `0x${string}`;
  data: `0x${string}`;
  value: `0x${string}`;
  maxFeePerGas: bigint;
  chainId: number;
} {
  return {
    to: tx.to as `0x${string}`,
    data: tx.data as `0x${string}`,
    value: '0x0' as const,
    maxFeePerGas: minFeePerGas(suggestedFee),
    chainId: tx.chain_id,
  };
}

/** True when the `build_tx` envelope indicates the protocol is paused. */
export function isPausedEnvelope(resp: {
  success: boolean;
  error?: { code: string; message: string };
}): boolean {
  return !resp.success && resp.error?.code === 'PAUSED';
}

/** True when the connected chain is Arc testnet. */
export function isChainAllowed(chainId: number | undefined): boolean {
  return chainId === ARC_CHAIN_ID;
}

// ── Permit2 signature splice ─────────────────────────────────────────

/**
 * The Rust `encode_permit2_pull` writes:
 *   - 6 ABI words (192 bytes = 384 hex chars) for the zeroed PermitSingle struct
 *   - 1 ABI word (32 bytes = 64 hex chars) for the signature offset (224 = 7*32)
 *   - 1 ABI word (32 bytes = 64 hex chars) for signature length
 *   - padded signature data (0 bytes when empty)
 *
 * Permit2Pull is the last thing in the splitSwap calldata.
 */
const PERMIT_SINGLE_HEX = 6 * 64; // 384 hex chars (192 bytes) — PermitSingle struct
const SIG_OFFSET_HEX = 64; // 64 hex chars (32 bytes) — offset to signature bytes
const SIG_LEN_HEX = 64; // 64 hex chars (32 bytes) — signature length
const EMPTY_TAIL_HEX = PERMIT_SINGLE_HEX + SIG_OFFSET_HEX + SIG_LEN_HEX; // 512 hex chars

/**
 * Pad a hex string to a 32-byte boundary (64 hex chars).
 * The Rust encoder uses: `vec![0u8; (32 - signature.len() % 32) % 32]`.
 */
function padTo32ByteBoundary(hex: string): string {
  const byteLen = hex.length / 2;
  const remainder = byteLen % 32;
  if (remainder === 0) return hex;
  const padBytes = 32 - remainder;
  return hex + '00'.repeat(padBytes);
}

/**
 * Splice a signed Permit2 signature into `build_tx.data`.
 *
 * T4.4 encodes `Permit2Pull.signature` as empty even when it returns
 * `typed_data`. After the user signs via `signTypedData`, we replace the
 * empty signature bytes in the calldata. Does NOT re-quote routes.
 *
 * When signature is already non-empty (non-zero sig_len), returns the
 * original data unchanged.
 */
export function spliceSignature(calldata: string, signature: string): string {
  if (!calldata.startsWith('0x')) return calldata;
  const selector = calldata.slice(2, 10);
  if (selector !== SPLIT_SWAP_SELECTOR) return calldata;

  const body = calldata.slice(10); // after 0x + selector
  if (body.length < EMPTY_TAIL_HEX) return calldata;

  // Read sig_len from the last 64 hex chars of body
  const sigLenHex = body.slice(body.length - SIG_LEN_HEX);
  const sigLen = parseInt(sigLenHex, 16);

  // If signature is already non-empty, don't overwrite
  if (sigLen > 0) return calldata;

  // Everything before the Permit2Pull tail (routes + other fixed data)
  const prefix = body.slice(0, body.length - EMPTY_TAIL_HEX);

  // Preserve the PermitSingle area (6 words) — the backend populates these
  // when typed_data is present (token, amount, expiration, nonce, spender,
  // sigDeadline). Only replace the signature bytes after them.
  const permitSingleArea = body.slice(
    body.length - EMPTY_TAIL_HEX,
    body.length - EMPTY_TAIL_HEX + PERMIT_SINGLE_HEX,
  );

  // Build new tail: existing PermitSingle + offset(224) + sig_len + sig_data
  const sigHex = signature.startsWith('0x') ? signature.slice(2) : signature;
  const sigLenBytes = sigHex.length / 2;
  const paddedSig = padTo32ByteBoundary(sigHex);
  const newTail =
    permitSingleArea + // Preserve populated PermitSingle words from backend
    (7 * 32).toString(16).padStart(SIG_OFFSET_HEX, '0') + // offset to signature (224)
    sigLenBytes.toString(16).padStart(SIG_LEN_HEX, '0') + // sig length (uint256)
    paddedSig; // sig data, padded to 32-byte boundary

  return '0x' + selector + prefix + newTail;
}

// ── ERC-20 approve calldata ──────────────────────────────────────────

/**
 * Build ERC-20 `approve(Permit2, amount)` calldata.
 * Used when `build_tx.required_approvals` is non-empty.
 */
export function encodeApproveCalldata(amount: string): string {
  const amountHex = BigInt(amount).toString(16).padStart(64, '0');
  return (
    '0x' +
    ERC20_APPROVE_SELECTOR +
    '000000000000000000000000' +
    '000000000022D473030F116dDEE9F6B43aC78BA3'.toLowerCase() +
    amountHex
  );
}

/**
 * Build Permit2 `permit(PermitSingle, signature)` calldata.
 * Called after EIP-712 signing when `typed_data` is non-null.
 */
export function encodePermitCalldata(permitSingleAbi: string, signature: string): string {
  const sigHex = signature.startsWith('0x') ? signature.slice(2) : signature;
  return '0x' + PERMIT2_PERMIT_SELECTOR + permitSingleAbi + padTo32ByteBoundary(sigHex);
}
