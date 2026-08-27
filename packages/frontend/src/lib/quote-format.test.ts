import { describe, expect, it } from 'vitest';
import { formatImpactPercent, formatProtocolFeePercent } from './quote-format';

describe('quote formatting (Chakra bps)', () => {
  it('formats price_impact_bps as a percent: 12 bps → 0.12%', () => {
    expect(formatImpactPercent(12)).toBe('0.12%');
    expect(formatImpactPercent(0)).toBe('0%');
    expect(formatImpactPercent(4)).toBe('0.04%');
  });

  it('protocol fee is always 0%', () => {
    expect(formatProtocolFeePercent(0)).toBe('0%');
  });
});
