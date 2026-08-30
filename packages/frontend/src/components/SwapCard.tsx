'use client';

import { useCallback, useMemo, useState } from 'react';
import { useWalletClient, usePublicClient } from 'wagmi';
import { buildSwapTx, type SubRoute } from '@/lib/aggregator';
import { useAccountBalances } from '@/lib/account-balances-context';
import { useWallet } from '@/lib/wallet-context';
import { useTokenCatalog, useGasPriceQuery, useQuoteQuery, type QuoteQueryKey } from '@/lib/hooks';
import { RouteDisplay } from './RouteDisplay';
import { TokenSelector } from './TokenSelector';
import { SwapSettingsModal } from './SwapSettingsModal';
import { UnauditedModal } from './UnauditedModal';
import { RecentSwaps } from './RecentSwaps';
import { formatErc20, formatNativeUsdc, slippageToBps, usdcMaxAtomic } from '@/lib/decimals';
import { formatImpactPercent } from '@/lib/quote-format';
import {
  DEFAULT_SWAP_SETTINGS,
  formatSlippageLabel,
  loadSwapSettings,
  saveSwapSettings,
  type SwapSettings,
} from '@/lib/swap-settings';
import { addRecentSwap, arcscanTxUrl } from '@/lib/recent-swaps';
import { hasAck } from '@/lib/unaudited-ack';
import {
  ARC_CHAIN_ID,
  fetchSuggestedFee,
  buildSendParams,
  isPausedEnvelope,
  isChainAllowed,
  spliceSignature,
} from '@/lib/swap-send';

const USDC_MAX_GAS_UNITS = 400_000n;

function amountInputToAtomic(amount: string, decimals: number): string {
  const base = 10n ** BigInt(decimals);
  const [whole, frac = ''] = amount.split('.');
  const w = whole === '' ? '0' : whole;
  const fraction = (frac || '').padEnd(decimals, '0').slice(0, decimals);
  const atomic = BigInt(w) * base + BigInt(fraction || '0');
  return atomic.toString();
}

function formatOutputAmount(atomic: string, decimals: number): string {
  const value = BigInt(atomic || '0');
  const base = 10n ** BigInt(decimals);
  const whole = value / base;
  const frac = value % base;
  if (frac === 0n) return whole.toString();
  const fracStr = frac.toString().padStart(decimals, '0').replace(/0+$/, '');
  const trimmed = fracStr.length > 8 ? fracStr.slice(0, 8) : fracStr;
  return `${whole}.${trimmed}`;
}

const VALID_INPUT_RE = /^\d*\.?\d*$/;

type SendState =
  | 'idle'
  | 'building'
  | 'approving'
  | 'permitting'
  | 'sending'
  | 'confirming'
  | 'confirmed'
  | 'error';

export function SwapCard() {
  const { address, onArcTestnet, connect, switchToArc } = useWallet();
  const {
    getErc20Balance,
    nativeBalance,
    loading: balancesLoading,
    ready: balancesReady,
  } = useAccountBalances();
  const { data: walletClient } = useWalletClient();
  const publicClient = usePublicClient();

  // Catalog (from query hook)
  const { tokens: catalogTokens } = useTokenCatalog();

  // Token selection by address, derived from shared catalog
  const [tokenInAddress, setTokenInAddress] = useState<string | null>(null);
  const [tokenOutAddress, setTokenOutAddress] = useState<string | null>(null);
  const [amountIn, setAmountIn] = useState('');
  const [settings, setSettings] = useState<SwapSettings>(DEFAULT_SWAP_SETTINGS);

  // Settings hydration (once on mount)
  const [settingsHydrated, setSettingsHydrated] = useState(false);
  if (!settingsHydrated) {
    setSettings(loadSwapSettings());
    setSettingsHydrated(true);
  }

  // Derive token objects from catalog (case-insensitive address match —
  // the API catalog is lowercased, FALLBACK_SWAP_TOKENS is mixed case).
  const tokenIn = useMemo(
    () =>
      catalogTokens.find((t) => t.address.toLowerCase() === tokenInAddress?.toLowerCase()) ?? null,
    [catalogTokens, tokenInAddress],
  );
  const tokenOut = useMemo(
    () =>
      catalogTokens.find((t) => t.address.toLowerCase() === tokenOutAddress?.toLowerCase()) ?? null,
    [catalogTokens, tokenOutAddress],
  );

  // Set defaults when catalog loads (once)
  const [defaultsSet, setDefaultsSet] = useState(false);
  if (!defaultsSet && catalogTokens.length > 0) {
    const first = catalogTokens[0];
    const second = catalogTokens.length > 1 ? catalogTokens[1] : undefined;
    if (!tokenInAddress && first) setTokenInAddress(first.address);
    if (!tokenOutAddress && second) setTokenOutAddress(second.address);
    setDefaultsSet(true);
  }

  // Quote query (debounced + polling via hook)
  const atomicAmount = useMemo(
    () => (tokenIn && amountIn ? amountInputToAtomic(amountIn, tokenIn.decimals) : ''),
    [tokenIn, amountIn],
  );

  const quoteKey: QuoteQueryKey | null = useMemo(
    () =>
      tokenIn && tokenOut && atomicAmount && atomicAmount !== '0'
        ? {
            tokenIn: tokenIn.address,
            tokenOut: tokenOut.address,
            amountIn: atomicAmount,
            slippageBps: slippageToBps(settings.slippage),
          }
        : null,
    [tokenIn, tokenOut, atomicAmount, settings.slippage],
  );

  const { data: quote, isLoading: quoteLoading, isStale: quoteStale } = useQuoteQuery(quoteKey);

  // Gas price
  const { data: gasPriceWei } = useGasPriceQuery();

  // Send pipeline state
  const [sendState, setSendState] = useState<SendState>('idle');
  const [txHash, setTxHash] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);

  // Settings modal
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Unaudited modal
  const [unauditedOpen, setUnauditedOpen] = useState(false);

  const balanceFor = useMemo(() => {
    if (!tokenIn) return null;
    return getErc20Balance(tokenIn.address);
  }, [tokenIn, getErc20Balance]);

  const swapDirection = useCallback(() => {
    setTokenInAddress(tokenOutAddress);
    setTokenOutAddress(tokenInAddress);
    setAmountIn('');
  }, [tokenInAddress, tokenOutAddress]);

  const applyBalancePercent = useCallback(
    (percent: number) => {
      if (balanceFor === null) return;
      if (percent >= 100) {
        if (gasPriceWei != null) {
          const gasCostWei = gasPriceWei * USDC_MAX_GAS_UNITS;
          const maxAtomic = usdcMaxAtomic(balanceFor, gasCostWei);
          setAmountIn(formatErc20(maxAtomic));
        } else {
          const floored = balanceFor > 100_000n ? balanceFor - 100_000n : 0n;
          setAmountIn(formatErc20(floored));
        }
      } else {
        const atomic = (balanceFor * BigInt(percent)) / 100n;
        setAmountIn(formatErc20(atomic));
      }
    },
    [balanceFor, gasPriceWei],
  );

  const humanError = useMemo(() => {
    if (sendError) {
      if (/no route/i.test(sendError)) return 'No route found for this pair';
      if (/unknown token/i.test(sendError)) return 'Unknown token';
      if (/zero amount|must be positive/i.test(sendError)) return 'Enter a valid amount';
      if (/rate limit/i.test(sendError)) return 'Rate limited — retry in a moment';
      if (/paused/i.test(sendError)) return 'Protocol is paused';
      return sendError;
    }
    if (quote === null && quoteKey && !quoteLoading) {
      return 'No route found';
    }
    return null;
  }, [sendError, quote, quoteKey, quoteLoading]);

  const primaryLabel = useMemo(() => {
    if (!address) return 'Connect Wallet';
    if (!onArcTestnet) return 'Switch to Arc Testnet';
    if (!amountIn || parseFloat(amountIn) <= 0) return 'Enter amount';
    if (quoteLoading) return 'Finding route…';
    if (quoteStale) return 'Finding route…';
    if (!quote) return 'No route available';
    if (sendState === 'building') return 'Building transaction…';
    if (sendState === 'approving') return 'Approve USDC';
    if (sendState === 'permitting') return 'Sign Permit2';
    if (sendState === 'sending') return 'Sending…';
    if (sendState === 'confirming') return 'Confirming…';
    if (sendState === 'confirmed') return 'Confirmed ✓';
    if (sendError?.includes('paused')) return 'Protocol paused';
    return 'Swap';
  }, [address, onArcTestnet, amountIn, quoteLoading, quoteStale, quote, sendState, sendError]);

  const primaryDisabled =
    !address ||
    !onArcTestnet ||
    !amountIn ||
    parseFloat(amountIn) <= 0 ||
    quoteLoading ||
    quoteStale ||
    !quote ||
    sendState === 'building' ||
    sendState === 'approving' ||
    sendState === 'permitting' ||
    sendState === 'sending' ||
    sendState === 'confirming' ||
    sendState === 'confirmed';

  const handlePrimary = useCallback(async () => {
    if (!address) {
      connect();
      return;
    }
    if (!onArcTestnet) {
      void switchToArc();
      return;
    }
    if (!quote || !tokenIn || !tokenOut || !amountIn || !walletClient || !publicClient) return;
    if (!isChainAllowed(walletClient.chain.id)) return;

    if (!hasAck()) {
      setUnauditedOpen(true);
      return;
    }

    setSendError(null);
    setSendState('building');

    try {
      const atomic = amountInputToAtomic(amountIn, tokenIn.decimals);
      const buildResult = await buildSwapTx(
        address,
        tokenIn.address,
        tokenOut.address,
        atomic,
        quote.minimum_output,
        quote.sub_routes,
      );

      if (!buildResult.success || !buildResult.data) {
        const msg = buildResult.error?.message || 'Failed to build transaction';
        if (isPausedEnvelope(buildResult)) {
          setSendError('Protocol is paused');
        } else {
          setSendError(msg);
        }
        setSendState('error');
        return;
      }

      const txData = buildResult.data;

      if (txData.required_approvals && txData.required_approvals.length > 0) {
        setSendState('approving');
        for (const approval of txData.required_approvals as Array<{
          token: string;
          spender: string;
          amount: string;
        }>) {
          const approveData = `0x095ea7b3${
            '000000000000000000000000' + approval.spender.toLowerCase().replace('0x', '')
          }${BigInt(approval.amount).toString(16).padStart(64, '0')}`;
          const approveHash = await walletClient.sendTransaction({
            to: approval.token as `0x${string}`,
            data: approveData as `0x${string}`,
            account: address,
            chain: walletClient.chain,
          });
          await publicClient.waitForTransactionReceipt({ hash: approveHash, confirmations: 1 });
        }
      }

      let calldata = txData.data;
      if (txData.typed_data) {
        setSendState('permitting');
        const typedData = txData.typed_data as {
          types: Record<string, Array<{ name: string; type: string }>>;
          primaryType: string;
          domain: Record<string, unknown>;
          message: Record<string, unknown>;
        };
        const sig = await walletClient.signTypedData({
          domain: typedData.domain as {
            name?: string;
            version?: string;
            chainId?: number;
            verifyingContract?: `0x${string}`;
          },
          types: typedData.types as Record<string, Array<{ name: string; type: string }>>,
          primaryType: typedData.primaryType as string,
          message: typedData.message as Record<string, unknown>,
          account: address,
        });
        calldata = spliceSignature(txData.data, sig);
      }

      setSendState('sending');
      const suggestedFee = await fetchSuggestedFee();
      const params = buildSendParams(txData, suggestedFee);
      const sendHash = await walletClient.sendTransaction({
        to: params.to,
        data: calldata as `0x${string}`,
        value: params.value === '0x0' ? 0n : BigInt(params.value),
        maxFeePerGas: params.maxFeePerGas,
        account: address,
        chain: walletClient.chain,
      });

      setTxHash(sendHash);

      setSendState('confirming');
      await publicClient.waitForTransactionReceipt({ hash: sendHash, confirmations: 1 });

      addRecentSwap(ARC_CHAIN_ID, address, {
        txHash: sendHash,
        tokenIn: tokenIn.symbol,
        tokenOut: tokenOut.symbol,
        amountIn: atomic,
        amountOut: quote.expected_output,
        isSplit: quote.is_split,
      });

      setSendState('confirmed');
      setTimeout(() => {
        setSendState('idle');
        setTxHash(null);
      }, 3000);
    } catch (err) {
      setSendState('error');
      setSendError(err instanceof Error ? err.message : 'Transaction failed');
    }
  }, [
    address,
    onArcTestnet,
    quote,
    tokenIn,
    tokenOut,
    amountIn,
    walletClient,
    publicClient,
    connect,
    switchToArc,
  ]);

  const handleUnauditedAck = useCallback(() => {
    setUnauditedOpen(false);
    void handlePrimary();
  }, [handlePrimary]);

  const handleSettingsChange = useCallback((next: SwapSettings) => {
    setSettings(next);
    saveSwapSettings(next);
  }, []);

  const subRoutes: SubRoute[] = quote?.sub_routes ?? [];

  return (
    <>
      <div className="w-full max-w-none space-y-3">
        <div className="surface-panel p-5 sm:p-6">
          <div className="flex items-center justify-between mb-5">
            <h2 className="text-[17px] sm:text-[18px] font-semibold tracking-tight text-[var(--text-primary)]">
              Swap
            </h2>
            <div className="flex items-center gap-2">
              <span className="text-[13px] text-[var(--text-muted)] tabular-nums font-[family-name:var(--font-mono)]">
                Slippage {formatSlippageLabel(settings.slippage)}
              </span>
              <button
                type="button"
                onClick={() => setSettingsOpen(true)}
                className="p-1.5 rounded-lg text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-0)] transition-colors"
                aria-label="Swap settings"
              >
                <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth={2}
                    d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
                  />
                  <circle cx="12" cy="12" r="3" strokeWidth={2} />
                </svg>
              </button>
            </div>
          </div>

          {nativeBalance !== null && (
            <div className="mb-3 text-[12px] text-[var(--text-muted)] font-[family-name:var(--font-mono)] tabular-nums">
              Gas (USDC): {formatNativeUsdc(nativeBalance)}
            </div>
          )}

          <div className="surface-panel-raised p-4 sm:p-5">
            <div className="flex justify-between items-center text-[13px] sm:text-[14px] text-[var(--text-muted)] mb-2.5 gap-2">
              <span>Sell</span>
              {address && tokenIn && (
                <span className="text-[var(--text-secondary)] truncate tabular-nums font-[family-name:var(--font-mono)] text-[13px]">
                  {balancesLoading && !balancesReady ? (
                    'Balance…'
                  ) : balanceFor !== null ? (
                    <>
                      {formatErc20(balanceFor)} {tokenIn.symbol}
                    </>
                  ) : (
                    '—'
                  )}
                </span>
              )}
            </div>
            <div className="flex items-center gap-3">
              <input
                type="text"
                inputMode="decimal"
                value={amountIn}
                onChange={(e) => {
                  const val = e.target.value;
                  if (VALID_INPUT_RE.test(val)) setAmountIn(val);
                }}
                placeholder="0.0"
                className="flex-1 bg-transparent text-[32px] sm:text-[36px] font-medium tracking-tight outline-none placeholder-[var(--text-muted)]/50 min-w-0 text-[var(--text-primary)] font-[family-name:var(--font-mono)]"
              />
              {tokenIn && (
                <TokenSelector
                  selected={tokenIn}
                  onSelect={(t) => setTokenInAddress(t.address)}
                  exclude={tokenOut?.address}
                />
              )}
            </div>
            {address && tokenIn && balanceFor !== null && (
              <div className="flex items-center gap-1.5 mt-3">
                {balanceFor > BigInt(0) ? (
                  <>
                    {[25, 50, 75].map((pct) => (
                      <button
                        key={pct}
                        type="button"
                        onClick={() => applyBalancePercent(pct)}
                        className="px-2.5 py-1 rounded-lg text-[13px] text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-0)] border border-transparent hover:border-[var(--border)] transition-colors"
                      >
                        {pct}%
                      </button>
                    ))}
                    <button
                      type="button"
                      onClick={() => applyBalancePercent(100)}
                      className="px-2.5 py-1 rounded-lg text-[13px] text-[var(--text-muted)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-0)] border border-transparent hover:border-[var(--border)] transition-colors"
                    >
                      Max
                    </button>
                  </>
                ) : (
                  <a
                    href="https://faucet.circle.com"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[12px] text-[var(--accent)] hover:underline"
                  >
                    Get {tokenIn.symbol} from Circle faucet
                  </a>
                )}
              </div>
            )}
          </div>

          <div className="flex justify-center -my-2.5 relative z-10">
            <button
              type="button"
              onClick={swapDirection}
              className="w-10 h-10 rounded-xl bg-[var(--bg-0)] border border-[var(--border)] flex items-center justify-center hover:border-[var(--border-strong)] hover:bg-[var(--surface-raised)] transition-colors group"
              aria-label="Swap token direction"
            >
              <svg
                className="w-4 h-4 text-[var(--text-muted)] group-hover:text-[var(--accent)] transition-colors"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                aria-hidden
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4"
                />
              </svg>
            </button>
          </div>

          <div className="surface-panel-raised p-4 sm:p-5">
            <div className="flex justify-between text-[13px] sm:text-[14px] text-[var(--text-muted)] mb-2.5">
              <span>Buy</span>
              {(quoteLoading || quoteStale) && (
                <span className="text-[var(--accent)]/80">Finding route…</span>
              )}
            </div>
            <div className="flex items-center gap-3">
              <div className="flex-1 text-[32px] sm:text-[36px] font-medium tracking-tight min-w-0 font-[family-name:var(--font-mono)]">
                {quote && tokenOut ? (
                  <span className="text-[var(--text-primary)]">
                    {formatOutputAmount(quote.expected_output, tokenOut.decimals)}
                  </span>
                ) : (
                  <span className="text-[var(--text-muted)]/60">0.0</span>
                )}
              </div>
              {tokenOut && (
                <TokenSelector
                  selected={tokenOut}
                  onSelect={(t) => setTokenOutAddress(t.address)}
                  exclude={tokenIn?.address}
                />
              )}
            </div>
          </div>

          {quote && (
            <div className="mt-3.5 px-0.5 space-y-1.5 text-[13px] sm:text-[14px] text-[var(--text-muted)]">
              <div className="flex items-center justify-between gap-3">
                <span>Price impact</span>
                <span className="tabular-nums font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
                  {formatImpactPercent(quote.price_impact_bps)}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span>Protocol fee</span>
                <span className="tabular-nums font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
                  {formatImpactPercent(quote.protocol_fee_bps)}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span>Minimum received</span>
                <span className="tabular-nums font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
                  {quote && tokenOut
                    ? formatOutputAmount(quote.minimum_output, tokenOut.decimals)
                    : '—'}{' '}
                  {tokenOut?.symbol ?? ''}
                </span>
              </div>
              <div className="flex items-center justify-between gap-3">
                <span>Route</span>
                <span className="tabular-nums font-[family-name:var(--font-mono)] text-[var(--text-secondary)]">
                  {subRoutes.length > 1
                    ? `${subRoutes.length} paths`
                    : (subRoutes[0]?.source ?? '—')}
                </span>
              </div>
            </div>
          )}

          {humanError && !quoteLoading && (
            <div className="mt-3 text-[13px] text-red-300/90 border border-red-500/15 bg-red-500/[0.05] rounded-xl px-3 py-2.5 text-center">
              {humanError}
            </div>
          )}

          {sendState === 'confirmed' && txHash && (
            <div className="mt-3 text-[13px] text-green-300/90 border border-green-500/15 bg-green-500/[0.05] rounded-xl px-3 py-2.5 text-center">
              Swap confirmed!{' '}
              <a
                href={arcscanTxUrl(txHash)}
                target="_blank"
                rel="noopener noreferrer"
                className="underline hover:text-green-200"
              >
                View on Arcscan
              </a>
            </div>
          )}

          <div className="mt-5">
            <button
              type="button"
              onClick={() => void handlePrimary()}
              disabled={primaryDisabled}
              className="btn-primary w-full py-4 min-h-[48px] text-[16px] sm:text-[17px]"
            >
              {primaryLabel}
            </button>
          </div>
        </div>

        {quote && tokenIn && tokenOut && (
          <RouteDisplay
            quote={quote}
            tokenInSymbol={tokenIn.symbol}
            tokenOutSymbol={tokenOut.symbol}
            tokenInDecimals={tokenIn.decimals}
            tokenOutDecimals={tokenOut.decimals}
            resolveTokenSymbol={(address: string) => {
              const t = catalogTokens.find(
                (tok) => tok.address.toLowerCase() === address.toLowerCase(),
              );
              return t?.symbol ?? `${address.slice(0, 4)}…${address.slice(-4)}`;
            }}
          />
        )}

        <RecentSwaps chainId={ARC_CHAIN_ID} address={address} />
      </div>

      <SwapSettingsModal
        open={settingsOpen}
        settings={settings}
        onClose={() => setSettingsOpen(false)}
        onChange={handleSettingsChange}
      />

      <UnauditedModal open={unauditedOpen} onAck={handleUnauditedAck} />
    </>
  );
}
