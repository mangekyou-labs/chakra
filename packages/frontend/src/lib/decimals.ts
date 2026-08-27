/** Chakra decimal helpers (SC-12). */

/** ERC-20 USDC (6 dp) — the swap token. */
export const USDC_ERC20_ADDRESS = '0x3600000000000000000000000000000000000000';
/** EURC (6 dp). */
export const EURC_ADDRESS = '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a';
/** Native USDC (18 dp) is gas only — never a swap token. */
export const NATIVE_USDC_KEY = 'native_usdc';

export const USDC_DECIMALS = 6;
export const EURC_DECIMALS = 6;
export const MBTC_DECIMALS = 8;
export const NATIVE_USDC_DECIMALS = 18;

/** 1 ERC-20 USDC atomic = 1e12 wei (6 vs 18 dp). */
const WEI_PER_ERC20_ATOMIC = 1_000_000_000_000n;
/** Floor on the USDC MAX gas buffer (0.10 USDC at 6 dp). */
const USDC_MAX_BUFFER_FLOOR = 100_000n;

/**
 * Rust `usdc_max_atomic` port (T1.2 / SC-12): reserve gas so a swap cannot
 * drain native USDC needed for the tx.
 * `raw = ceil(gas_cost_wei / 1e12)`; `buffer = max(ceil(raw * 1.25), 100_000)`.
 */
export function usdcMaxAtomic(erc20Balance6dp: bigint, gasCostWei: bigint): bigint {
  const raw =
    gasCostWei % WEI_PER_ERC20_ATOMIC === 0n
      ? gasCostWei / WEI_PER_ERC20_ATOMIC
      : gasCostWei / WEI_PER_ERC20_ATOMIC + 1n;
  const withMargin = (raw * 125n) % 100n === 0n ? (raw * 125n) / 100n : (raw * 125n) / 100n + 1n;
  const buffer = withMargin > USDC_MAX_BUFFER_FLOOR ? withMargin : USDC_MAX_BUFFER_FLOOR;
  const result = erc20Balance6dp - buffer;
  return result > 0n ? result : 0n;
}

/** Native USDC / ETH / zero address encodings are never swap tokens. */
export function isNativeSwapToken(address: string): boolean {
  const a = address.toLowerCase();
  return a === NATIVE_USDC_KEY || a === 'eth' || a === '0x0000000000000000000000000000000000000000';
}

/** Format an ERC-20 atomic (6 dp) without floats or scientific notation. */
export function formatErc20(atomic: string | bigint): string {
  return formatAtomic(atomic, 6);
}

/** Format native USDC atomic (18 dp) without floats or scientific notation. */
export function formatNativeUsdc(atomic: string | bigint): string {
  return formatAtomic(atomic, 18);
}

function formatAtomic(atomic: string | bigint, decimals: number): string {
  const value = typeof atomic === 'bigint' ? atomic : BigInt(atomic || '0');
  const base = 10n ** BigInt(decimals);
  const whole = value / base;
  const frac = value % base;
  if (frac === 0n) return whole.toString();
  const fracStr = frac.toString().padStart(decimals, '0').replace(/0+$/, '');
  return `${whole}.${fracStr}`;
}

/** Percent slippage (0.5) → integer bps (50). */
export function slippageToBps(slippage: number): number {
  return Math.round(slippage * 100);
}
