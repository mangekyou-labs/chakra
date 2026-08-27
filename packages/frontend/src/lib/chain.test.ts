import { describe, expect, it } from 'vitest';
import {
  ARC_CHAIN_ID,
  ARC_CHAIN_ID_HEX,
  ARC_ADD_CHAIN_PARAMS,
  isArcTestnet,
  nativeGasSymbol,
} from './chain';

describe('isArcTestnet', () => {
  it('accepts Arc testnet chain id 5042002', () => {
    expect(isArcTestnet(5042002)).toBe(true);
  });

  it('rejects other chain ids and undefined', () => {
    expect(isArcTestnet(1)).toBe(false);
    expect(isArcTestnet(14)).toBe(false);
    expect(isArcTestnet(114)).toBe(false);
    expect(isArcTestnet(undefined as unknown as number)).toBe(false);
  });
});

describe('ARC_ADD_CHAIN_PARAMS', () => {
  it('pins the Arc testnet chain id as 0x4CEF52', () => {
    expect(ARC_CHAIN_ID).toBe(5042002);
    expect(ARC_CHAIN_ID_HEX).toBe('0x4CEF52');
    expect(ARC_ADD_CHAIN_PARAMS.chainId).toBe('0x4CEF52');
  });

  it('uses USDC 18 dp native currency, public Arc RPC, and Arcscan explorer', () => {
    expect(ARC_ADD_CHAIN_PARAMS.nativeCurrency).toEqual({
      name: 'USDC',
      symbol: 'USDC',
      decimals: 18,
    });
    expect(ARC_ADD_CHAIN_PARAMS.rpcUrls).toEqual(['https://rpc.testnet.arc.io']);
    expect(ARC_ADD_CHAIN_PARAMS.blockExplorerUrls).toEqual(['https://testnet.arcscan.app']);
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
