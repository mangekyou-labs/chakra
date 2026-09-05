/**
 * MetaMask 13 prompt helpers for the headed T11.10 harness.
 *
 * dAppwright's `closeWalletSetupPopup` auto-closes stray `home.html` tabs
 * without a query string. Connection/tx confirmations live on
 * `notification.html` (or `home.html` with a request query). Keep
 * `wallet.page` on the extension home and drive the DApp from a separate tab.
 */
import type { BrowserContext, Page } from '@playwright/test';

/** dAppwright remaps the unpacked MetaMask extension to this id. */
export const METAMASK_EXTENSION_ID = 'gadekpdjmpjjnnemgnhkbjgnjpdaakgh';

export function metamaskNotificationUrl(): string {
  return `chrome-extension://${METAMASK_EXTENSION_ID}/notification.html`;
}

export function isMetaMaskPromptUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== 'chrome-extension:') return false;
    const path = parsed.pathname.toLowerCase();
    if (path.endsWith('/notification.html') || path.endsWith('/popup.html')) return true;
    if (
      path.endsWith('/home.html') &&
      (parsed.search.length > 1 || /connect|confirm|permission|snaps|review/i.test(parsed.hash))
    ) {
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

export function confirmActionLocator(page: Page) {
  return page
    .locator('[data-testid="confirm-footer-button"]')
    .or(page.locator('[data-testid="confirm-btn"]'))
    .or(page.locator('[data-testid="page-container-footer-next"]'))
    .or(page.locator('[data-testid="allow-authorize-button"]'))
    .or(page.getByRole('button', { name: /^(connect|approve|confirm|next|sign|connect anyway|i understand)$/i }))
    .or(page.getByRole('button', { name: /^connect /i }));
}

export function dumpContextPages(context: BrowserContext, label: string): void {
  const urls = context
    .pages()
    .map((page) => (page.isClosed() ? 'closed' : page.url()))
    .join(' | ');
  console.log(`[${label}] pages: ${urls || '(none)'}`);
}

export async function waitForInjectedProvider(page: Page, timeoutMs = 20_000): Promise<void> {
  await page.waitForFunction(() => Boolean((window as { ethereum?: unknown }).ethereum), {
    timeout: timeoutMs,
  });
}

async function collectPromptPages(context: BrowserContext, walletPage: Page): Promise<Page[]> {
  const open = context.pages().filter((page) => !page.isClosed());
  const fromUrl = open.filter((page) => isMetaMaskPromptUrl(page.url()));
  if (fromUrl.length > 0) return fromUrl;

  if (!walletPage.isClosed()) {
    const inTab = confirmActionLocator(walletPage).first();
    if (await inTab.isVisible({ timeout: 200 }).catch(() => false)) {
      return [walletPage];
    }
  }
  return [];
}

/**
 * Click through MetaMask 13 connect / add-chain / sign / confirm prompts until
 * `done()` is true. Opens `notification.html` once if no prompt page appears.
 */
export async function confirmMetaMaskPromptsUntil(
  context: BrowserContext,
  options: {
    walletPage: Page;
    done: () => Promise<boolean>;
    timeoutMs?: number;
  },
): Promise<void> {
  const timeoutMs = options.timeoutMs ?? 45_000;
  const started = Date.now();
  const deadline = started + timeoutMs;
  let openedNotification = false;
  let clicks = 0;
  let idleTicks = 0;

  while (Date.now() < deadline) {
    if (await options.done()) return;

    const prompts = await collectPromptPages(context, options.walletPage);
    if (prompts.length === 0) {
      idleTicks += 1;
      if (idleTicks === 1 || idleTicks % 6 === 0) dumpContextPages(context, 'metamask-idle');
      if (!openedNotification && Date.now() - started > 3_000) {
        openedNotification = true;
        console.log('[metamask] opening notification.html fallback');
        const notification = await context.newPage();
        await notification.goto(metamaskNotificationUrl(), { waitUntil: 'domcontentloaded' }).catch((err) => {
          console.log(`[metamask] notification.html goto failed: ${err}`);
        });
      }
      await options.walletPage.waitForTimeout(500);
      continue;
    }

    for (const prompt of prompts) {
      await prompt.bringToFront().catch(() => {});

      const reviewAlert = prompt.getByRole('button', { name: /review alert/i });
      if (await reviewAlert.isVisible({ timeout: 300 }).catch(() => false)) {
        console.log(`[metamask] review alert on ${prompt.url()}`);
        await reviewAlert.click();
      }
      const risk = prompt.getByRole('checkbox');
      if (await risk.isVisible({ timeout: 300 }).catch(() => false)) {
        await risk.check().catch(() => {});
      }
      const gotIt = prompt.getByRole('button', { name: /got it/i });
      if (await gotIt.isVisible({ timeout: 300 }).catch(() => false)) {
        await gotIt.click();
      }

      const scrollBtn = prompt.locator('[data-testid="signature-request-scroll-button"]');
      if (await scrollBtn.isVisible({ timeout: 300 }).catch(() => false)) {
        await scrollBtn.click().catch(() => {});
      }

      const action = confirmActionLocator(prompt).first();
      if (await action.isVisible({ timeout: 800 }).catch(() => false)) {
        const label = ((await action.innerText().catch(() => 'action')) || 'action').trim();
        console.log(`[metamask] clicking '${label}' on ${prompt.url()}`);
        await action.click({ timeout: 5_000 }).catch((err) => {
          console.log(`[metamask] click failed: ${err}`);
        });
        clicks += 1;
        await prompt.waitForTimeout(400);
      }
    }
    await options.walletPage.waitForTimeout(400);
  }

  if (await options.done()) return;
  dumpContextPages(context, 'metamask-timeout');
  throw new Error(`MetaMask prompt did not complete after ${timeoutMs}ms (${clicks} clicks)`);
}
