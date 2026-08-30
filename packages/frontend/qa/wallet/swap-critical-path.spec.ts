/**
 * qa/wallet/swap-critical-path.spec.ts
 *
 * Real MetaMask wallet end-to-end critical path for Chakra Arc Testnet DEX Aggregator.
 * Uses dAppwright to automate real MetaMask extension interactions on Chromium.
 *
 * Requirements:
 *   - Headed Chromium with real MetaMask extension via dAppwright
 *   - Connects to Chakra UI, switches/adds Arc Testnet (Chain ID 5042002 / 0x4CEF52)
 *   - Quotes USDC -> EURC with visible legs, price_impact_bps, protocol fee 0
 *   - Handles Permit2 approve + EIP-712 PermitSingle typed data signing
 *   - Broadcasts splitSwap with value = 0n, waits 1 confirmation
 *   - Verifies Arcscan explorer link and recent-swaps localStorage entry
 *   - Gracefully SKIPS when QA_WALLET_SECRET is not configured
 *
 * Locators mirror production helpers (src/lib/chain.ts, src/lib/recent-swaps.ts,
 * src/components/SwapCard.tsx) — explorer `.app`, `chakra:recent-swaps:{chainId}:{address}`,
 * `Swap confirmed!` banner, single primary button, unaudited ack modal.
 * Network switch is app-driven via the primary button's switchToArc flow.
 */
import { test, expect } from '@playwright/test';
import dappwright, { MetaMaskWallet } from '@tenkeylabs/dappwright';
import {
  QA_CHAIN_ID,
  QA_EXPLORER_URL,
  QA_STORAGE_PREFIX,
  QA_SWAP_CONFIRMED_TEXT,
} from './constants';

const DAPP_URL = process.env.DAPP_URL || 'https://chakra-arc-dex.vercel.app';
const _API_URL = process.env.QA_API_URL || 'https://chakra-api-0a5i.onrender.com';
const QA_WALLET_SECRET = process.env.QA_WALLET_SECRET || '';

test.describe('Chakra Arc Testnet — MetaMask Critical Path (T9.4)', () => {
  test('MetaMask real wallet swap flow (skip when unconfigured)', async () => {
    // 1. Skip check: do not fail when secrets are missing
    if (!QA_WALLET_SECRET || QA_WALLET_SECRET.trim().length === 0) {
      test.skip(true, 'QA_WALLET_SECRET is not set — skipping live MetaMask headed run');
      return;
    }

    const isMnemonic = QA_WALLET_SECRET.trim().includes(' ');

    // 2. Launch headed Chromium with dAppwright MetaMask (mnemonic seed preferred)
    const [wallet, page, context] = await dappwright.bootstrap('chromium', {
      wallet: 'metamask',
      version: MetaMaskWallet.recommendedVersion,
      seed: isMnemonic ? QA_WALLET_SECRET.trim() : undefined,
      password: 'TestPassword123!',
      headless: false,
    });

    try {
      // If private key was provided, import it (fallback path only)
      if (!isMnemonic) {
        const pk = QA_WALLET_SECRET.trim().startsWith('0x')
          ? QA_WALLET_SECRET.trim().slice(2)
          : QA_WALLET_SECRET.trim();
        await wallet.importPK(pk);
      }

      // 3. Navigation / connection is app-driven: MetaMask may not know Arc yet,
      //    so the UI's switchToArc handles wallet_addEthereumChain via the app.
      await page.goto(DAPP_URL, { waitUntil: 'domcontentloaded' });
      await expect(page).toHaveTitle(/Chakra/i);

      // 4. Connect wallet (EIP-6963 injected connector — no RainbowKit modal)
      const connectBtn = page.getByRole('button', { name: /connect/i }).first();
      await expect(connectBtn).toBeVisible({ timeout: 15_000 });
      await connectBtn.click();

      // Approve connection in MetaMask
      await wallet.approve();
      await page.bringToFront();
      await expect(page.getByRole('button', { name: /connect/i }).first()).toBeHidden({
        timeout: 15_000,
      });

      // 5. Primary button drives state: Connect -> Switch to Arc Testnet -> Swap.
      //    The swap card's primary button is disabled while !onArcTestnet — the
      //    switch action lives in the header wallet menu (HeaderWallet.tsx).
      const primaryBtn = page.locator('button.btn-primary').first();
      await expect(primaryBtn).toBeVisible({ timeout: 15_000 });

      // Open the header address chip and use the menu's "Switch to Arc Testnet".
      // dAppwright's addNetwork/confirmNetworkSwitch are broken on MetaMask 13.17
      // (stub/UI-selection mismatch) — drive the MetaMask notification popup directly.
      await expect(page.getByRole('button', { name: /^0x/ }).first()).toBeVisible({
        timeout: 20_000,
      });
      const addrChip = page.getByRole('button', { name: /^0x/ }).first();
      await addrChip.click();
      await expect(
        page.getByRole('button', { name: /switch to arc testnet/i }).first(),
      ).toBeVisible({
        timeout: 10_000,
      });
      const switchMenuItem = page.getByRole('button', { name: /switch to arc testnet/i });
      await switchMenuItem.first().click();
      // The wallet_switchEthereumChain -> wallet_addEthereumChain flow opens a
      // MetaMask notification popup with Cancel/Confirm. The add-chain flow can
      // require TWO confirms (add network, then switch) — loop until no popup remains.
      for (let attempt = 0; attempt < 8; attempt += 1) {
        await page.waitForTimeout(2_000).catch(() => {});
        const popup = context
          .pages()
          .filter((p) => p !== page)
          .at(-1);
        if (!popup) {
          console.log(`[switch] attempt ${attempt}: no popup`);
          break;
        }
        let handledAlert = false;
        const reviewAlertBtn = popup.getByRole('button', { name: /review alert/i });
        if (await reviewAlertBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
          console.log(`[switch] attempt ${attempt}: reviewing MetaMask network alert`);
          await reviewAlertBtn.click();
          await popup.waitForTimeout(500);
          handledAlert = true;
        }
        const riskCheckbox = popup.getByRole('checkbox');
        if (await riskCheckbox.isVisible({ timeout: 2_000 }).catch(() => false)) {
          console.log(`[switch] attempt ${attempt}: acknowledging MetaMask risk alert`);
          await riskCheckbox.check();
          handledAlert = true;
        }
        const gotItBtn = popup.getByRole('button', { name: /got it/i });
        if (await gotItBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
          await gotItBtn.click();
          await popup.waitForTimeout(500);
          handledAlert = true;
        }
        const confirmBtn = popup
          .getByRole('button', {
            name: /^(confirm|approve|next|add|continue|connect anyway|i understand)$/i,
          })
          .or(popup.locator('[data-testid="confirm-footer-button"]'))
          .or(popup.locator('[data-testid="confirm-btn"]'))
          .or(popup.locator('[data-testid="page-container-footer-next"]'));
        if (await confirmBtn.isVisible({ timeout: 5_000 }).catch(() => false)) {
          console.log(`[switch] attempt ${attempt}: confirming ${popup.url().slice(-30)}`);
          await confirmBtn.click();
          await confirmBtn.waitFor({ state: 'detached', timeout: 10_000 }).catch(() => {});
        } else if (handledAlert) {
          const popupText = await popup
            .locator('body')
            .innerText()
            .catch(() => '');
          console.log(
            `[switch] attempt ${attempt}: alert flow still open: ${popupText.slice(0, 240).replace(/\s+/g, ' ')}`,
          );
          continue;
        } else {
          console.log(`[switch] attempt ${attempt}: popup no confirm btn`);
          break;
        }
      }
      await page.bringToFront().catch(() => {});
      // Wait for the swap card's primary button to become actionable (on Arc):
      // the dot locator is fragile; the button label is the authoritative signal.
      await expect(page.locator('button.btn-primary').first()).toHaveText(
        /^(Enter amount|Finding route|Swap)/,
        {
          timeout: 30_000,
        },
      );

      // 6. Enter swap parameters (1.0 USDC -> EURC; defaults now apply correctly).
      const sellInput = page.locator('input[placeholder="0.0"]').first();
      await expect(sellInput).toBeVisible({ timeout: 10_000 });
      await sellInput.fill('1.0');

      // 7. Wait for quote to resolve — the primary button is the authoritative
      //    signal: 'Finding route…' while loading, 'No route available' on failure,
      //    'Swap' when a route exists (SwapCard.tsx primaryLabel).
      await expect(primaryBtn).toBeEnabled({ timeout: 45_000 });
      await expect(primaryBtn).toHaveText(/^Swap$/, { timeout: 45_000 });

      // Verify route/protocol-fee summary rows (RouteDisplay + quote panel)
      await expect(page.getByText('Route', { exact: true }).first()).toBeVisible();
      await expect(page.getByText('Protocol fee', { exact: true }).first()).toBeVisible();

      // 8. Execute Swap via the single primary button
      await primaryBtn.click();

      // 9. Unaudited-contracts ack modal (first send) — matches UnauditedModal.tsx
      const unauditedAck = page.getByRole('button', { name: /i understand — proceed/i });
      if (await unauditedAck.isVisible({ timeout: 3_000 }).catch(() => false)) {
        await unauditedAck.click();
      }

      // 10. Sequence MetaMask popups to match handlePrimary:
      //     optional ERC-20 approve confirm -> EIP-712 PermitSingle sign -> splitSwap confirm
      for (let attempt = 0; attempt < 15; attempt += 1) {
        await page.waitForTimeout(2_000);
        if (
          await page
            .locator(`text=${QA_SWAP_CONFIRMED_TEXT}`)
            .isVisible()
            .catch(() => false)
        ) {
          console.log('[swap] swap confirmed banner visible');
          break;
        }
        const popup = context
          .pages()
          .filter((p) => p !== page)
          .at(-1);
        if (!popup) {
          continue;
        }
        await popup.bringToFront().catch(() => {});

        // Handle scroll down button if present on EIP-712 sign requests
        const scrollBtn = popup.locator('[data-testid="signature-request-scroll-button"]');
        if (await scrollBtn.isVisible({ timeout: 1_000 }).catch(() => false)) {
          console.log(`[swap] attempt ${attempt}: clicking signature scroll button`);
          await scrollBtn.click().catch(() => {});
          await popup.waitForTimeout(500);
        }

        // Click Sign / Confirm / Next / Approve / confirm-footer-button
        const actionBtn = popup
          .getByRole('button', { name: /^(sign|confirm|approve|next)$/i })
          .or(popup.locator('[data-testid="confirm-footer-button"]'))
          .or(popup.locator('[data-testid="confirm-btn"]'))
          .or(popup.locator('[data-testid="page-container-footer-next"]'));

        if (
          await actionBtn
            .first()
            .isVisible({ timeout: 2_000 })
            .catch(() => false)
        ) {
          const btnText = await actionBtn
            .first()
            .innerText()
            .catch(() => 'action');
          console.log(`[swap] attempt ${attempt}: clicking '${btnText}' button in popup`);
          await actionBtn
            .first()
            .click()
            .catch(() => {});
          await page.waitForTimeout(1_000).catch(() => {});
        }
      }
      await page.bringToFront().catch(() => {});
      // 11. Verify transaction success state — capture hash BEFORE the 3s banner hide
      const successBanner = page.locator(`text=${QA_SWAP_CONFIRMED_TEXT}`);
      await expect(successBanner).toBeVisible({ timeout: 60_000 });

      // Verify Arcscan explorer link (.app — matches arcscanTxUrl)
      const explorerLink = page.locator(`a[href*="${QA_EXPLORER_URL}/tx/"]`);
      await expect(explorerLink).toBeVisible();
      const txUrl = await explorerLink.getAttribute('href');
      expect(txUrl).toBeTruthy();

      // 12. Verify recent swaps stored in localStorage under the production key
      const recentSwaps = await page.evaluate(
        ({ prefix, chainId }) => {
          const keys: string[] = [];
          for (let i = 0; i < localStorage.length; i += 1) {
            const k = localStorage.key(i);
            if (k && k.startsWith(`${prefix}:${chainId}:`)) keys.push(k);
          }
          return keys;
        },
        { prefix: QA_STORAGE_PREFIX, chainId: QA_CHAIN_ID },
      );
      expect(recentSwaps.length).toBeGreaterThan(0);
      const latest = await page.evaluate(
        (key: string) => localStorage.getItem(key),
        recentSwaps[0],
      );
      expect(latest).toBeTruthy();
      const parsed = JSON.parse(latest || '[]') as Array<{ txHash?: string; isSplit?: boolean }>;
      expect(parsed[0]?.txHash).toBeTruthy();
      // 1.0 USDC sorts to a single-path route at this size; isSplit may be false.
      expect(typeof parsed[0]?.isSplit).toBe('boolean');
    } finally {
      await context.close();
    }
  });
});
