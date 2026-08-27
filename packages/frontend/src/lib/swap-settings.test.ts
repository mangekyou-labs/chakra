import { describe, expect, it } from 'vitest';
import {
  DEFAULT_SWAP_SETTINGS,
  SWAP_SETTINGS_STORAGE_KEY,
  loadSwapSettings,
} from './swap-settings';

describe('swap settings (chakra)', () => {
  it('defaults slippage to 0.5%', () => {
    expect(DEFAULT_SWAP_SETTINGS.slippage).toBe(0.5);
  });

  it('uses the chakra storage key', () => {
    expect(SWAP_SETTINGS_STORAGE_KEY).toBe('chakra:swap-settings');
  });

  it('loads 0.5 default when nothing stored', () => {
    expect(loadSwapSettings().slippage).toBe(0.5);
  });
});
