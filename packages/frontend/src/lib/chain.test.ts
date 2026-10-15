import { describe, expect, it } from 'vitest';
import {
  ARC_CHAIN_ID,
  ARC_CHAIN_ID_HEX,
  ARC_ADD_CHAIN_PARAMS,
  isArcMainnet,
  nativeGasSymbol,
} from './chain';

describe('isArcMainnet', () => {
  it('accepts Arc mainnet chain id 5042', () => {
    expect(isArcMainnet(5042)).toBe(true);
  });

  it('rejects other chain ids and undefined', () => {
    expect(isArcMainnet(1)).toBe(false);
    expect(isArcMainnet(14)).toBe(false);
    expect(isArcMainnet(114)).toBe(false);
    expect(isArcMainnet(undefined as unknown as number)).toBe(false);
  });
});

describe('ARC_ADD_CHAIN_PARAMS', () => {
  it('pins the Arc mainnet chain id as 0x13B2', () => {
    expect(ARC_CHAIN_ID).toBe(5042);
    expect(ARC_CHAIN_ID_HEX).toBe('0x13B2');
    expect(ARC_ADD_CHAIN_PARAMS.chainId).toBe('0x13B2');
  });

  it('uses USDC 18 dp native currency, public Arc RPC, and Arcscan explorer', () => {
    expect(ARC_ADD_CHAIN_PARAMS.nativeCurrency).toEqual({
      name: 'USDC',
      symbol: 'USDC',
      decimals: 18,
    });
    expect(ARC_ADD_CHAIN_PARAMS.rpcUrls).toEqual(['https://rpc.mainnet.arc.io']);
    expect(ARC_ADD_CHAIN_PARAMS.blockExplorerUrls).toEqual(['https://explorer.arc.io']);
  });
});

describe('nativeGasSymbol', () => {
  it('always reports USDC even when the wallet labels the native asset ETH', () => {
    expect(nativeGasSymbol('ETH')).toBe('USDC');
    expect(nativeGasSymbol('native')).toBe('USDC');
    expect(nativeGasSymbol('USDC')).toBe('USDC');
    expect(nativeGasSymbol(undefined)).toBe('USDC');
  });
});
