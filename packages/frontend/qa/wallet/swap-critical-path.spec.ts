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
import {
  confirmMetaMaskPromptsUntil,
  dumpContextPages,
  waitForInjectedProvider,
} from './metamask-prompt';

const DAPP_URL = process.env.DAPP_URL || 'https://chakra-ag.vercel.app';
const _API_URL = process.env.QA_API_URL || 'https://chakra-api-0a5i.onrender.com';
const QA_WALLET_SECRET = process.env.QA_WALLET_SECRET || '';

test.describe('Chakra Arc Testnet — MetaMask Critical Path (T9.4)', () => {
  test('MetaMask real wallet swap flow (skip when unconfigured)', async () => {
    test.setTimeout(360_000);
    // 1. Skip check: do not fail when secrets are missing
    if (!QA_WALLET_SECRET || QA_WALLET_SECRET.trim().length === 0) {
      test.skip(true, 'QA_WALLET_SECRET is not set — skipping live MetaMask headed run');
      return;
    }

    const isMnemonic = QA_WALLET_SECRET.trim().includes(' ');

    // 2. Launch headed Chromium with dAppwright MetaMask (mnemonic seed preferred).
    // Use launch + explicit setup instead of bootstrap so the SRP flow can
    // commit MetaMask 13.17's final recovery word explicitly.
    const { wallet, browserContext: context } = await dappwright.launch('chromium', {
      wallet: 'metamask',
      version: MetaMaskWallet.recommendedVersion,
      headless: false,
    });
    const walletPage = wallet.page;
    // Keep the extension home tab intact. Navigating wallet.page to the DApp
    // destroys MetaMask's UI and races dAppwright's stray-home closer, which
    // left the previous headed run stuck on "Connecting…".

    await wallet.setup(
      {
        seed: isMnemonic ? QA_WALLET_SECRET.trim() : undefined,
        password: 'TestPassword123!',
      },
      [
        async (metamaskPage, options) => {
          if (!options?.seed) return;
          await metamaskPage.getByTestId('onboarding-import-wallet').click();
          await metamaskPage.getByTestId('onboarding-import-with-srp-button').click();
          await metamaskPage
            .getByTestId('srp-input-import__srp-note')
            .pressSequentially(options.seed.trim(), { delay: 30 });
          // MetaMask keeps the final word uncommitted until a separator is
          // pressed; without this the Continue button remains disabled.
          await metamaskPage.keyboard.press('Space');
          await metamaskPage.getByTestId('import-srp-confirm').click();
        },
        async (metamaskPage, options) => {
          await metamaskPage
            .getByTestId('create-password-new-input')
            .fill(options?.password || 'TestPassword123!');
          await metamaskPage
            .getByTestId('create-password-confirm-input')
            .fill(options?.password || 'TestPassword123!');
          await metamaskPage.getByTestId('create-password-terms').click();
          await metamaskPage.getByTestId('create-password-submit').click();
        },
        async (metamaskPage) => {
          await metamaskPage.getByTestId('metametrics-checkbox').click();
          await metamaskPage.getByTestId('metametrics-i-agree').click();
          await metamaskPage.getByTestId('manage-default-settings').click();
          await metamaskPage.getByTestId('category-item-General').click();
          await metamaskPage.getByTestId('backup-and-sync-toggle-container').click();
          await metamaskPage.getByTestId('category-back-button').click();
          await metamaskPage.getByTestId('privacy-settings-back-button').click();
          await metamaskPage.getByTestId('onboarding-complete-done').click();
        },
      ],
    );

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
      const page = await context.newPage();
      await page.goto(DAPP_URL, { waitUntil: 'domcontentloaded' });
      await expect(page).toHaveTitle(/Chakra/i);
      await waitForInjectedProvider(page);

      // 4. Connect wallet (EIP-6963 injected connector — no RainbowKit modal)
      const connectBtn = page.getByRole('button', { name: /connect/i }).first();
      await expect(connectBtn).toBeVisible({ timeout: 15_000 });
      dumpContextPages(context, 'pre-connect');
      await connectBtn.click();
      await confirmMetaMaskPromptsUntil(context, {
        walletPage,
        timeoutMs: 45_000,
        done: async () =>
          page
            .getByRole('button', { name: /connect/i })
            .first()
            .isHidden()
            .catch(() => false),
      });
      await page.bringToFront();
      await expect(page.getByRole('button', { name: /connect/i }).first()).toBeHidden({
        timeout: 10_000,
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
      const onArcAlready = await page
        .locator('button.btn-primary')
        .first()
        .innerText()
        .then((text) => /^(Enter amount|Finding route|Swap)/.test(text.trim()))
        .catch(() => false);
      if (!onArcAlready) {
        const addrChip = page.getByRole('button', { name: /^0x/ }).first();
        await addrChip.click();
        await expect(
          page.getByRole('button', { name: /switch to arc testnet/i }).first(),
        ).toBeVisible({
          timeout: 10_000,
        });
        await page.getByRole('button', { name: /switch to arc testnet/i }).first().click();
        // wallet_switchEthereumChain -> wallet_addEthereumChain can take two confirms.
        await confirmMetaMaskPromptsUntil(context, {
          walletPage,
          timeoutMs: 60_000,
          done: async () => {
            const text = (
              await page.locator('button.btn-primary').first().innerText().catch(() => '')
            ).trim();
            return /^(Enter amount|Finding route|Swap)/.test(text);
          },
        });
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
      await confirmMetaMaskPromptsUntil(context, {
        walletPage,
        timeoutMs: 120_000,
        done: async () =>
          page
            .locator(`text=${QA_SWAP_CONFIRMED_TEXT}`)
            .isVisible()
            .catch(() => false),
      });
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
