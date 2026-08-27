import { describe, it, expect } from 'vitest';
import {
  minFeePerGas,
  buildSendParams,
  isPausedEnvelope,
  isChainAllowed,
  spliceSignature,
  ARC_CHAIN_ID,
  MIN_FEE_PER_GAS_WEI,
} from './swap-send';

describe('swap-send', () => {
  describe('minFeePerGas', () => {
    it('returns MIN_FEE_PER_GAS_WEI when suggested is lower', () => {
      expect(minFeePerGas(BigInt(1e9))).toBe(MIN_FEE_PER_GAS_WEI);
    });
    it('returns suggested when it exceeds minimum', () => {
      expect(minFeePerGas(BigInt(30e9))).toBe(BigInt(30e9));
    });
    it('returns minimum when suggested is exactly at boundary', () => {
      expect(minFeePerGas(MIN_FEE_PER_GAS_WEI)).toBe(MIN_FEE_PER_GAS_WEI);
    });
  });

  describe('buildSendParams', () => {
    const baseData = {
      to: '0xAggregatorAddress',
      data: '0x2e3be0c1',
      chain_id: ARC_CHAIN_ID,
      value: '0',
      deadline: Math.floor(Date.now() / 1000) + 120,
      typed_data: null,
      required_approvals: [],
    };
    it('sets value to 0x0 always', () => {
      expect(buildSendParams(baseData, BigInt(25e9)).value).toBe('0x0');
    });
    it('sets maxFeePerGas from suggested when above min', () => {
      expect(buildSendParams(baseData, BigInt(30e9)).maxFeePerGas).toBe(BigInt(30e9));
    });
    it('enforces minFeePerGas when suggested is low', () => {
      expect(buildSendParams(baseData, BigInt(1e9)).maxFeePerGas).toBe(MIN_FEE_PER_GAS_WEI);
    });
    it('returns correct to and data', () => {
      const p = buildSendParams(baseData, BigInt(25e9));
      expect(p.to).toBe('0xAggregatorAddress');
      expect(p.data).toBe('0x2e3be0c1');
    });
  });

  describe('isPausedEnvelope', () => {
    it('returns true for PAUSED', () => {
      expect(isPausedEnvelope({ success: false, error: { code: 'PAUSED', message: '' } })).toBe(
        true,
      );
    });
    it('returns false for NO_ROUTE', () => {
      expect(isPausedEnvelope({ success: false, error: { code: 'NO_ROUTE', message: '' } })).toBe(
        false,
      );
    });
    it('returns false for success', () => {
      expect(isPausedEnvelope({ success: true })).toBe(false);
    });
  });

  describe('isChainAllowed', () => {
    it('allows Arc testnet', () => {
      expect(isChainAllowed(ARC_CHAIN_ID)).toBe(true);
    });
    it('rejects other chains', () => {
      expect(isChainAllowed(1)).toBe(false);
      expect(isChainAllowed(undefined)).toBe(false);
    });
  });

  describe('spliceSignature', () => {
    // 128 hex chars of head data (64 bytes)
    const HEAD = 'aa'.repeat(64);
    // 384 hex chars PermitSingle zeros (6 × 32 bytes)
    const PERMIT_SINGLE = '00'.repeat(192);
    // 64 hex chars sig_offset = 224 (7*32, pointing to sig_len)
    const SIG_OFFSET = (7 * 32).toString(16).padStart(64, '0');
    // 64 hex chars sig_len = 0 (1 × 32 bytes)
    const SIG_LEN_ZEROS = '0'.repeat(64);

    function buildOrigData(): string {
      return '0x2e3be0c1' + HEAD + PERMIT_SINGLE + SIG_OFFSET + SIG_LEN_ZEROS;
    }

    it('splices empty signature into calldata', () => {
      const origData = buildOrigData();
      const sig = '0x' + 'ab'.repeat(65); // 65 bytes = 130 hex chars
      const spliced = spliceSignature(origData, sig);

      expect(spliced).not.toBe(origData);
      expect(spliced.startsWith('0x2e3be0c1')).toBe(true);

      const body = spliced.slice(10); // after 0x + selector

      // Head preserved (128 hex chars)
      expect(body.slice(0, 128)).toBe(HEAD);

      // PermitSingle area preserved (384 hex chars)
      expect(body.slice(128, 128 + 384)).toBe(PERMIT_SINGLE);

      // sig_len at offset 128 (head) + 384 (PermitSingle) + 64 (offset) = 576
      const sigLenOffset = 576;
      const sigLen = BigInt('0x' + body.slice(sigLenOffset, sigLenOffset + 64));
      expect(sigLen).toBe(65n);

      // sig data follows
      const sigData = body.slice(sigLenOffset + 64);
      expect(sigData.startsWith('ab'.repeat(65))).toBe(true);
    });

    it('does not overwrite non-empty signature', () => {
      // sig_len = 0x41 = 65 (non-zero)
      const sigLenNonZero = '0'.repeat(31) + '41';
      const origData = '0x2e3be0c1' + HEAD + PERMIT_SINGLE + SIG_OFFSET + sigLenNonZero;
      const spliced = spliceSignature(origData, '0x' + 'cd'.repeat(65));
      expect(spliced).toBe(origData);
    });

    it('preserves populated PermitSingle words when splicing signature', () => {
      // Simulate backend-populated PermitSingle: 6 × 64 hex chars (192 bytes = 384 hex chars)
      // Each field right-aligned in its own 32-byte (64 hex char) word.
      const populatedPermitSingle =
        '000000000000000000000000' +
        'aa'.repeat(20) + // token (address, right-aligned)
        '00000000000000000000000000000000000000000000000000000000000f4240' + // amount = 1_000_000
        '0000000000000000000000000000000000000000000000000000006600000000' + // expiration (uint48)
        '000000000000000000000000000000000000000000000000000000000000002a' + // nonce = 42
        '000000000000000000000000' +
        'bb'.repeat(20) + // spender (address, right-aligned)
        '00000000000000000000000000000000000000000000000000000000000f4240'; // sigDeadline
      const sig = '0x' + 'ab'.repeat(65);
      const origData = '0x2e3be0c1' + HEAD + populatedPermitSingle + SIG_OFFSET + SIG_LEN_ZEROS;
      const spliced = spliceSignature(origData, sig);

      const body = spliced.slice(10);
      // PermitSingle area must be preserved exactly
      expect(body.slice(128, 128 + 384)).toBe(populatedPermitSingle);
      // sig_len should be 65
      const sigLen = BigInt('0x' + body.slice(576, 576 + 64));
      expect(sigLen).toBe(65n);
      // sig data should follow
      expect(body.slice(640).startsWith('ab'.repeat(65))).toBe(true);
    });

    it('returns original when data is too short', () => {
      const shortData = '0x2e3be0c1' + '00'.repeat(10);
      expect(spliceSignature(shortData, '0x' + 'ab'.repeat(65))).toBe(shortData);
    });

    it('returns original when selector is wrong', () => {
      const data = '0xdeadbeef' + '00'.repeat(200);
      expect(spliceSignature(data, '0x' + 'ab'.repeat(65))).toBe(data);
    });
  });
});
