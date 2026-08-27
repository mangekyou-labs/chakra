import { describe, expect, it } from 'vitest';
import { filterSwapTokens, type SwapToken } from './swap-tokens';

const USDC: SwapToken = {
  address: '0x3600000000000000000000000000000000000000',
  symbol: 'USDC',
  name: 'USD Coin',
  decimals: 6,
};

const EURC: SwapToken = {
  address: '0x89B50855aa3be2f677cd6303cec089b5f319d72a',
  symbol: 'EURC',
  name: 'Euro Coin',
  decimals: 6,
};

describe('filterSwapTokens', () => {
  it('keeps catalog ERC-20 tokens and drops native encodings', () => {
    const nativeRows = [
      { address: 'native_usdc', symbol: 'USDC', name: 'Native USDC', decimals: 18 },
      {
        address: '0x0000000000000000000000000000000000000000',
        symbol: 'ETH',
        name: 'ETH',
        decimals: 18,
      },
      { address: 'eth', symbol: 'ETH', name: 'Ether', decimals: 18 },
    ];
    const result = filterSwapTokens([...nativeRows, USDC, EURC]);
    expect(result.map((t) => t.symbol)).toEqual(['USDC', 'EURC']);
  });

  it('falls back to the hardcoded USDC+EURC catalog when the API row is missing', () => {
    const result = filterSwapTokens([]);
    expect(result.map((t) => t.symbol)).toEqual(['USDC', 'EURC']);
    expect(result[0].decimals).toBe(6);
  });
});
