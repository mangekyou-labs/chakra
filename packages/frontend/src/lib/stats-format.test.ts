import { describe, expect, it } from 'vitest';
import {
  chartGeometry,
  formatBpsPercent,
  formatCompactCount,
  formatMicrosUsd,
  formatRefreshAge,
  formatUsdCompact,
  groupThousands,
  isStatsRange,
  microsBigInt,
  parseStatsRange,
  statsTokenLabel,
} from './stats-format';

describe('formatMicrosUsd (BigInt money)', () => {
  it('renders 1,000,000 micros as $1.00 — not "1M"', () => {
    expect(formatMicrosUsd('1000000')).toBe('$1.00');
  });

  it('renders zero honestly', () => {
    expect(formatMicrosUsd('0')).toBe('$0.00');
    expect(formatMicrosUsd('')).toBe('$0.00');
  });

  it('half-up rounds to the cent', () => {
    expect(formatMicrosUsd('123456789')).toBe('$123.46');
    expect(formatMicrosUsd('123455000')).toBe('$123.46');
    expect(formatMicrosUsd('123454999')).toBe('$123.45');
  });

  it('groups large dollar amounts without floating point', () => {
    expect(formatMicrosUsd('2500000000000')).toBe('$2,500,000.00');
  });

  it('never renders sub-cent amounts as 1M-style units', () => {
    expect(formatMicrosUsd('50')).toBe('$0.00');
  });

  it('treats API junk as zero instead of throwing', () => {
    expect(formatMicrosUsd('not-a-number')).toBe('$0.00');
  });
});

describe('formatUsdCompact (chart labels)', () => {
  it('stays full at low values', () => {
    expect(formatUsdCompact('1000000')).toBe('$1.00');
    expect(formatUsdCompact('0')).toBe('$0.00');
  });
  it('compacts only above $1,000', () => {
    expect(formatUsdCompact('1200000000')).toBe('$1.2K');
    expect(formatUsdCompact('1500000000000000')).toBe('$1.5B');
  });

  it('treats API junk as zero instead of throwing', () => {
    expect(formatUsdCompact('not-a-number')).toBe('$0.00');
  });
});

describe('formatCompactCount', () => {
  it('groups small counts', () => {
    expect(formatCompactCount(999)).toBe('999');
    expect(formatCompactCount(1234)).toBe('1.2K');
  });
  it('compacts million counts to one decimal', () => {
    expect(formatCompactCount(1234567)).toBe('1.2M');
    expect(formatCompactCount(2000000)).toBe('2M');
  });
  it('handles billion counts', () => {
    expect(formatCompactCount(12000000000)).toBe('12B');
  });
});

describe('formatBpsPercent', () => {
  it('maps integer bps to trimmed percentages', () => {
    expect(formatBpsPercent(500)).toBe('5%');
    expect(formatBpsPercent(0)).toBe('0%');
    expect(formatBpsPercent(1234)).toBe('12.34%');
    expect(formatBpsPercent(5)).toBe('0.05%');
    expect(formatBpsPercent(1250)).toBe('12.5%');
  });
});

describe('formatRefreshAge', () => {
  it('renders seconds, minutes, and hours', () => {
    expect(formatRefreshAge(45)).toBe('45s');
    expect(formatRefreshAge(130)).toBe('2m 10s');
    expect(formatRefreshAge(3600)).toBe('1h');
    expect(formatRefreshAge(3720)).toBe('1h 2m');
  });
});

describe('range helpers', () => {
  it('validates and parses the documented ranges', () => {
    expect(isStatsRange('14d')).toBe(true);
    expect(isStatsRange('all')).toBe(true);
    expect(isStatsRange('7d')).toBe(false);
    expect(parseStatsRange('?range=90d')).toBe('90d');
    expect(parseStatsRange('?range=7d')).toBe('30d');
    expect(parseStatsRange('')).toBe('30d');
    expect(parseStatsRange(null)).toBe('30d');
  });
});

describe('groupThousands', () => {
  it('groups plain digit strings', () => {
    expect(groupThousands('1234567')).toBe('1,234,567');
    expect(groupThousands('123')).toBe('123');
  });
});

describe('microsBigInt', () => {
  it('parses decimal strings and tolerates junk', () => {
    expect(microsBigInt('9007199254740993')).toBe(9007199254740993n);
    expect(microsBigInt(null)).toBe(0n);
    expect(microsBigInt('not-a-number')).toBe(0n);
  });
});

describe('chartGeometry (BigInt scale before Number)', () => {
  const point = (micros: string, day = '2026-09-04') => ({
    day,
    stablecoin_notional_micros: micros,
    swaps: 1,
  });

  it('returns empty points for an empty series', () => {
    expect(chartGeometry([])).toEqual({ points: '' });
  });

  it('places a single nonzero point at the left pad and top of the plot', () => {
    expect(chartGeometry([point('1000000')])).toEqual({
      points: '',
      onePoint: { x: 10, y: 12 },
    });
  });

  it('puts a max=0 series on the baseline', () => {
    expect(chartGeometry([point('0')])).toEqual({
      points: '',
      onePoint: { x: 10, y: 138 },
    });
    expect(chartGeometry([point('0'), point('0')])).toEqual({
      points: '10,138 790,138',
    });
  });

  it('scales a multi-point series with BigInt division before Number', () => {
    const huge = '1000000000000000000';
    const half = '500000000000000000';
    expect(chartGeometry([point('0', 'a'), point(half, 'b'), point(huge, 'c')])).toEqual({
      points: '10,138 400,75 790,12',
    });
  });
});

describe('statsTokenLabel', () => {
  it('uses the canonical cirBTC spelling', () => {
    expect(statsTokenLabel('0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF')).toBe('cirBTC');
  });
  it('labels USDC and EURC and shortens unknown addresses', () => {
    expect(statsTokenLabel('0x3600000000000000000000000000000000000000')).toBe('USDC');
    expect(statsTokenLabel('0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a')).toBe('EURC');
    expect(statsTokenLabel('0xabcdef')).toBe('0xabcd…');
  });
});
