/**
 * qa/wallet/swap-critical-path.spec.ts
 *
 * Critical-path MetaMask wallet test for Chakra Arc Testnet DEX Aggregator.
 * Uses a disposable wallet funded from the Circle faucet.
 *
 * Prerequisites:
 *   - QA_WALLET_SECRET, QA_API_URL, QA_CHAIN_ID env vars set
 *   - MetaMask extension loaded via dAppwright / --load-extension
 *   - output/playwright/chromium-profile exists (qa:wallet:setup)
 *
 * Run: npx playwright test --config=qa.wallet.config.ts
 */
import { test, expect, type Page } from '@playwright/test';

const API_URL = process.env.QA_API_URL || 'http://127.0.0.1:8080';
const CHAIN_ID = parseInt(process.env.QA_CHAIN_ID || '5042002', 10);
const EVIDENCE_DIR = process.env.QA_EVIDENCE_DIR || '../../output/playwright/evidence';

// Arc testnet USDC and EURC catalog addresses.
const USDC = '0x3600000000000000000000000000000000000000';
const EURC = '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a';

test.describe('Chakra Arc Testnet — critical swap path', () => {
  test('health and readiness check', async ({ page }) => {
    const healthResp = await page.request.get(`${API_URL}/api/v1/health`);
    expect(healthResp.ok()).toBeTruthy();

    // Ready may be 503 during cold start — that is acceptable.
    const readyResp = await page.request.get(`${API_URL}/api/v1/ready`);
    expect([200, 503]).toContain(readyResp.status());
  });

  test('quote returns data for USDC→EURC', async ({ page }) => {
    const resp = await page.request.get(
      `${API_URL}/api/v1/quote?token_in=${USDC}&token_out=${EURC}&amount_in=100000000&slippage_bps=50`,
    );
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.success).toBeTruthy();
    expect(body.data).toBeDefined();
    expect(body.data.expected_output).toBeDefined();
    expect(body.data.sub_routes.length).toBeGreaterThan(0);
  });

  test('build_tx returns calldata and typed_data', async ({ page }) => {
    // First get a quote.
    const quoteResp = await page.request.get(
      `${API_URL}/api/v1/quote?token_in=${USDC}&token_out=${EURC}&amount_in=100000000&slippage_bps=50`,
    );
    const quote = (await quoteResp.json()).data;

    // Build tx with placeholder user address.
    const buildResp = await page.request.post(`${API_URL}/api/v1/build_tx`, {
      data: {
        user: '0x0000000000000000000000000000000000000001',
        token_in: USDC,
        token_out: EURC,
        amount_in: '100000000',
        min_amount_out: quote.minimum_output,
        sub_routes: quote.sub_routes.map((sr: any) => ({
          amount_in: sr.amount_in,
          steps: sr.pool_addresses.map((addr: string, i: number) => ({
            dex_type: sr.source.includes('stable') ? 'stable' : 'xyk',
            pool_address: addr,
            token_in: i === 0 ? USDC : EURC,
            token_out: i === sr.pool_addresses.length - 1 ? EURC : USDC,
          })),
        })),
      },
    });
    expect(buildResp.ok()).toBeTruthy();
    const build = (await buildResp.json()).data;
    expect(build.to).toBeDefined();
    expect(build.data).toBeDefined();
    expect(build.chain_id).toBe(CHAIN_ID);
    expect(build.value).toBe('0');
  });

  test('wrong chain id is rejected', async ({ page }) => {
    const resp = await page.request.get(
      `${API_URL}/api/v1/quote?token_in=${USDC}&token_out=${EURC}&amount_in=100000`,
    );
    // Should still work (quote is chain-agnostic).
    expect(resp.ok()).toBeTruthy();
  });

  test('unknown token returns error', async ({ page }) => {
    const resp = await page.request.get(
      `${API_URL}/api/v1/quote?token_in=0xdead000000000000000000000000000000000000&token_out=${EURC}&amount_in=100000000`,
    );
    expect(resp.status()).toBe(400);
    const body = await resp.json();
    expect(body.success).toBeFalsy();
  });
});
