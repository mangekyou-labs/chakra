import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

function parseHexColor(hex: string): [number, number, number] {
  const clean = hex.replace('#', '').trim();
  if (clean.length === 3) {
    return [
      parseInt(clean[0] + clean[0], 16),
      parseInt(clean[1] + clean[1], 16),
      parseInt(clean[2] + clean[2], 16),
    ];
  }
  return [
    parseInt(clean.slice(0, 2), 16),
    parseInt(clean.slice(2, 4), 16),
    parseInt(clean.slice(4, 6), 16),
  ];
}

function sRgbToLinear(c: number): number {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
}

function relativeLuminance(r: number, g: number, b: number): number {
  const R = sRgbToLinear(r);
  const G = sRgbToLinear(g);
  const B = sRgbToLinear(b);
  return 0.2126 * R + 0.7152 * G + 0.0722 * B;
}

function contrastRatio(hex1: string, hex2: string): number {
  const [r1, g1, b1] = parseHexColor(hex1);
  const [r2, g2, b2] = parseHexColor(hex2);
  const l1 = relativeLuminance(r1, g1, b1);
  const l2 = relativeLuminance(r2, g2, b2);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

describe('Theme WCAG AA contrast compliance (T9.8)', () => {
  const globalsCssPath = path.resolve(__dirname, '../app/globals.css');
  const cssContent = fs.readFileSync(globalsCssPath, 'utf8');

  function getCssVar(name: string): string {
    const match = cssContent.match(new RegExp(`${name}:\\s*([^;]+);`));
    if (!match) throw new Error(`Variable ${name} not found in globals.css`);
    return match[1].trim();
  }

  it('verifies --text-muted meets WCAG AA 4.5:1 contrast against --bg-0 and --surface-raised', () => {
    const textMuted = getCssVar('--text-muted');
    const bg0 = getCssVar('--bg-0');
    const surfaceRaised = getCssVar('--surface-raised');

    const contrastBg0 = contrastRatio(textMuted, bg0);
    const contrastSurfaceRaised = contrastRatio(textMuted, surfaceRaised);

    // WCAG AA requires at least 4.5:1 for normal body text
    expect(
      contrastBg0,
      `--text-muted (${textMuted}) vs --bg-0 (${bg0}) contrast ${contrastBg0.toFixed(2)} must be >= 4.5:1`
    ).toBeGreaterThanOrEqual(4.5);

    expect(
      contrastSurfaceRaised,
      `--text-muted (${textMuted}) vs --surface-raised (${surfaceRaised}) contrast ${contrastSurfaceRaised.toFixed(2)} must be >= 4.5:1`
    ).toBeGreaterThanOrEqual(4.5);
  });
});
