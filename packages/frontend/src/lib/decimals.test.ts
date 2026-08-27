import { describe, expect, it } from 'vitest';
import {
  formatErc20,
  formatNativeUsdc,
  isNativeSwapToken,
  slippageToBps,
  usdcMaxAtomic,
  USDC_ERC20_ADDRESS,
} from './decimals';

describe('usdcMaxAtomic (Rust port pin)', () => {
  // 0 wei gas still leaves the 100_000 (0.10 USDC) floor.
  it('floors at 100_000 atomic when gas is 0', () => {
    expect(usdcMaxAtomic(1_000_000n, 0n)).toBe(900_000n);
  });

  it('2e12 wei → raw 2, ×1.25 = 3, floor still dominates', () => {
    expect(usdcMaxAtomic(500_000n, 2_000_000_000_000n)).toBe(400_000n);
  });

  it('1 wei over a 1e12 boundary ceils to raw 2', () => {
    expect(usdcMaxAtomic(1_000_000n, 1_000_000_000_001n)).toBe(900_000n);
  });

  it('1.25× margin dominates the floor when gas is large', () => {
    const gas = 100_000n * 1_000_000_000_000n;
    expect(usdcMaxAtomic(1_000_000n, gas)).toBe(875_000n);
  });

  it('saturates to 0 when the balance is below the buffer', () => {
    expect(usdcMaxAtomic(50_000n, 0n)).toBe(0n);
  });
});

describe('decimal formatting split (6/8 vs 18)', () => {
  it('formats ERC-20 (6 dp) atomics without scientific notation', () => {
    expect(formatErc20('1234567890')).toBe('1234.56789');
    expect(formatErc20('1000000')).toBe('1');
    expect(formatErc20('0')).toBe('0');
  });

  it('formats native USDC (18 dp) with its own scale', () => {
    expect(formatNativeUsdc('99000000000000000000')).toBe('99');
    expect(formatNativeUsdc('99000000000000000001')).toBe('99.000000000000000001');
  });
});

describe('native USDC never a swap token', () => {
  it('rejects native encodings', () => {
    expect(isNativeSwapToken('native_usdc')).toBe(true);
    expect(isNativeSwapToken('eth')).toBe(true);
    expect(isNativeSwapToken('0x0000000000000000000000000000000000000000')).toBe(true);
  });

  it('accepts the ERC-20 catalog entry', () => {
    expect(isNativeSwapToken(USDC_ERC20_ADDRESS)).toBe(false);
  });
});

describe('slippageToBps', () => {
  it('converts 0.5% → 50 bps', () => {
    expect(slippageToBps(0.5)).toBe(50);
    expect(slippageToBps(1)).toBe(100);
    expect(slippageToBps(0.01)).toBe(1);
  });
});
