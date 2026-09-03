#!/usr/bin/env node
/**
 * qa/swap/smoke.mjs — Chakra production smoke: atomic USDC → cirBTC.
 *
 * A viem-based QA command that mirrors the production wallet swap path
 * (SwapCard.tsx + swap-send.ts): quote → route checks → build_tx →
 * exact-amount ERC-20 approvals → Permit2 EIP-712 signature → calldata
 * splice → simulate → (only with --broadcast) submit and wait for a receipt.
 *
 * Contract
 *   - QA_WALLET_SECRET is read ONLY from the environment (never from a file),
 *     and never printed. It may be a 12/24-word mnemonic or a private key
 *     (0x-prefixed or raw hex).
 *   - Defaults to DRY-RUN: nothing is broadcast, no approvals are sent.
 *   - Requires an explicit --broadcast flag to send transactions.
 *   - Aborts when the quote is not the canonical USDC → EURC → cirBTC
 *     multihop through the UnitFlow cirBTC pool, or price impact exceeds
 *     100 bps.
 *
 * Usage
 *   QA_WALLET_SECRET="…" node qa/swap/smoke.mjs [--broadcast]
 *     [--amount-in 1000000] [--slippage-bps 50] [--api URL] [--rpc URL]
 *
 * Exit codes: 0 = plan OK (dry-run) / swap confirmed (broadcast);
 *             1 = route, build, or network failure;
 *             2 = funding blocker or preflight failure (nothing broadcast).
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createPublicClient, createWalletClient, http } from 'viem';
import { mnemonicToAccount, privateKeyToAccount } from 'viem/accounts';

// ── Arc testnet + catalog constants (mirror src/lib/chain.ts, decimals.ts) ──

const ARC_CHAIN_ID = 5042002;
const DEFAULT_RPC_URL = 'https://rpc.testnet.arc.io';
const DEFAULT_API_URL = 'https://chakra-api-0a5i.onrender.com';

/** Token catalog (addresses are lowercase in the API transport). */
const USDC = '0x3600000000000000000000000000000000000000'; // 6 dp
const EURC = '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a'; // 6 dp
const CIRBTC = '0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF'; // 8 dp
const UNITFLOW_CIRBTC_POOL = '0x268DC75517EaFc6e0D52666639529e5DAB8c9200'; // EURC/cirBTC
const ERC20_APPROVE_SELECTOR = '095ea7b3';
const MIN_FEE_PER_GAS_WEI = 20n * 10n ** 9n; // 20 gwei floor (swap-send.ts)
const EXPLORER_URL = 'https://testnet.arcscan.app';

const ARC_CHAIN = {
  id: ARC_CHAIN_ID,
  name: 'Arc Testnet',
  nativeCurrency: { name: 'USDC', symbol: 'USDC', decimals: 18 },
  rpcUrls: { default: { http: [DEFAULT_RPC_URL] } },
};

const ERC20_ABI = [
  {
    type: 'function',
    name: 'balanceOf',
    stateMutability: 'view',
    inputs: [{ name: 'account', type: 'address' }],
    outputs: [{ name: '', type: 'uint256' }],
  },
];

// ── CLI parsing ──────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const flags = {
  broadcast: args.includes('--broadcast'),
  amountIn: '1000000', // 1 USDC atomic by default
  slippageBps: 50,
};
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === '--amount-in') flags.amountIn = args[++i];
  if (args[i] === '--slippage-bps') flags.slippageBps = Number(args[++i]);
  if (args[i] === '--api') flags.apiUrl = args[++i];
  if (args[i] === '--rpc') flags.rpcUrl = args[++i];
  if (args[i] === '--help' || args[i] === '-h') {
    console.log(`Chakra QA swap smoke — atomic USDC → EURC → cirBTC.
  QA_WALLET_SECRET must be exported (mnemonic or private key). Never read
  from a file, never printed.
  Flags:
    --broadcast       Send approvals + swap (default is a dry run).
    --amount-in N     Atomic USDC input (default 1000000).
    --slippage-bps N  Slippage in bps (default 50).
    --api URL         Chakra API base (default ${DEFAULT_API_URL}).
    --rpc URL         Arc RPC (default ${DEFAULT_RPC_URL}).`);
    process.exit(0);
  }
}

const API_URL = (flags.apiUrl || process.env.QA_API_URL || DEFAULT_API_URL).replace(/\/$/, '');
const RPC_URL = flags.rpcUrl || process.env.QA_RPC_URL || DEFAULT_RPC_URL;
const MODE = flags.broadcast ? 'BROADCAST' : 'DRY-RUN';
const nowIso = () => new Date().toISOString();

// ── Env-only secret loading ──────────────────────────────────────────────────

function loadSecret() {
  const secret = process.env.QA_WALLET_SECRET;
  if (!secret || !secret.trim()) {
    console.error('❌ QA_WALLET_SECRET is not set. Export it (mnemonic or private key):');
    console.error('   export QA_WALLET_SECRET="…"');
    process.exit(1);
  }
  const value = secret.trim();
  if (/\s/.test(value)) {
    const words = value.split(/\s+/);
    if (![12, 24].includes(words.length)) {
      console.error(`❌ Mnemonic must be 12 or 24 words (got ${words.length})`);
      process.exit(1);
    }
    console.log(`   Wallet secret: mnemonic (${words.length} words)`);
    return { account: mnemonicToAccount(value), kind: 'mnemonic' };
  }
  const key = value.startsWith('0x') ? value : `0x${value}`;
  if (!/^0x[0-9a-fA-F]{64}$/.test(key)) {
    console.error('❌ Private key must be 32 bytes of hex (0x-prefixed or raw)');
    process.exit(1);
  }
  console.log('   Wallet secret: private key (never printed)');
  return { account: privateKeyToAccount(key), kind: 'private-key' };
}

// ── viem clients ─────────────────────────────────────────────────────────────

function clients(account) {
  const publicClient = createPublicClient({ chain: ARC_CHAIN, transport: http(RPC_URL) });
  const walletClient = createWalletClient({ account, chain: ARC_CHAIN, transport: http(RPC_URL) });
  return { publicClient, walletClient };
}

// ── Minimal envelope fetch helpers (mirror src/lib/aggregator.ts) ────────────

async function apiFetch(path, init) {
  const res = await fetch(`${API_URL}${path}`, init);
  let json;
  try {
    json = await res.json();
  } catch {
    throw new Error(`API ${res.status} returned non-JSON from ${path}`);
  }
  if (!res.ok && !json?.success) {
    throw new Error(`API ${res.status} ${path}: ${json?.error?.message || 'request failed'}`);
  }
  return json;
}

async function getQuote(tokenIn, tokenOut, amountIn) {
  const params = new URLSearchParams({
    token_in: tokenIn,
    token_out: tokenOut,
    amount_in: amountIn,
    slippage_bps: String(flags.slippageBps),
  });
  const json = await apiFetch(`/api/v1/quote?${params}`);
  if (!json.success || !json.data) {
    throw new Error(
      `Quote failed: ${json.error?.code || 'NO_ROUTE'} — ${json.error?.message || 'no route'}`,
    );
  }
  return json.data;
}

function buildTxSubRoutes(subRoutes) {
  return subRoutes.map((sr) => ({
    amount_in: sr.amount_in,
    steps: sr.pool_addresses.map((pool, i) => {
      const step = {
        dex_type: sr.dex_types?.[i] ?? 'xyk',
        pool_address: pool,
        token_in: sr.path[i] ?? '',
        token_out: sr.path[i + 1] ?? '',
      };
      const fee = sr.hop_fees?.[i];
      if (fee !== undefined && fee > 0) step.fee_bps = fee;
      return step;
    }),
  }));
}

async function buildSwapTx(user, tokenIn, tokenOut, amountIn, minAmountOut, subRoutes) {
  const json = await apiFetch('/api/v1/build_tx', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user,
      token_in: tokenIn,
      token_out: tokenOut,
      amount_in: amountIn,
      min_amount_out: minAmountOut,
      sub_routes: buildTxSubRoutes(subRoutes),
    }),
  });
  if (!json.success || !json.data) {
    throw new Error(
      `build_tx failed: ${json.error?.code || 'ROUTE_INVALID'} — ${json.error?.message || ''}`,
    );
  }
  return json.data;
}

// ── Canonical route enforcement ──────────────────────────────────────────────

const ci = (a) => String(a).toLowerCase();

function assertCanonicalRoute(quote) {
  const { sub_routes: subs, price_impact_bps: impact, is_split } = quote;
  const problems = [];
  if (is_split)
    problems.push(`quote is split (is_split=true) — refusing non-canonical split routing`);
  if (!Array.isArray(subs) || subs.length !== 1) {
    problems.push(`expected exactly 1 subroute, got ${Array.isArray(subs) ? subs.length : 'none'}`);
  } else {
    const sr = subs[0];
    const path = (sr.path || []).map(ci);
    const pools = (sr.pool_addresses || []).map(ci);
    const wantPath = [ci(USDC), ci(EURC), ci(CIRBTC)];
    if (path.length !== 3 || path.some((p, i) => p !== wantPath[i])) {
      problems.push(
        `path is not the canonical USDC → EURC → cirBTC multihop (got ${sr.path?.join(' → ') || 'empty'})`,
      );
    }
    if (pools.length !== 2 || pools[1] !== ci(UNITFLOW_CIRBTC_POOL)) {
      problems.push(
        `hop-2 pool is not the UnitFlow EURC/cirBTC pool ${UNITFLOW_CIRBTC_POOL} (got ${sr.pool_addresses?.join(', ') || 'empty'})`,
      );
    }
    if (sr.source && !/xylo|presto|xyk/i.test(sr.source)) {
      problems.push(
        `subroute source "${sr.source}" is not a discovered Xylo/Presto/UnitFlow venue`,
      );
    }
    if (!sr.amount_out || BigInt(sr.amount_out) <= 0n) problems.push('subroute output is zero');
  }
  if (typeof impact !== 'number' || impact > 100) {
    problems.push(`price impact ${impact} bps exceeds the 100 bps abort threshold`);
  }
  if (!quote.minimum_output || BigInt(quote.minimum_output) <= 0n)
    problems.push('minimum output is zero');
  if (problems.length) {
    throw new Error(`Route abort: ${problems.join('; ')}`);
  }
}

// ── Calldata helpers (mirror swap-send.ts) ───────────────────────────────────

function padTo32ByteBoundary(hex) {
  const byteLen = hex.length / 2;
  const remainder = byteLen % 32;
  if (remainder === 0) return hex;
  return hex + '00'.repeat(32 - remainder);
}

/**
 * Replace the empty Permit2 signature inside splitSwap calldata with the
 * signed EIP-712 signature (mirror of spliceSignature in swap-send.ts).
 */
function spliceSignature(calldata, signature) {
  if (!calldata.startsWith('0x') || calldata.slice(2, 10) !== '2e3be0c1') return calldata;
  const body = calldata.slice(10);
  const TAIL = 6 * 64 + 64 + 64; // PermitSingle + offset + sig len (512 hex chars)
  if (body.length < TAIL) return calldata;
  const sigLen = parseInt(body.slice(body.length - 64), 16);
  if (sigLen > 0) return calldata; // already signed — never overwrite
  const prefix = body.slice(0, body.length - TAIL);
  const permitSingleArea = body.slice(body.length - TAIL, body.length - TAIL + 6 * 64);
  const sigHex = signature.startsWith('0x') ? signature.slice(2) : signature;
  const sigLenBytes = sigHex.length / 2;
  const newTail =
    permitSingleArea +
    (7 * 32).toString(16).padStart(64, '0') + // signature offset (224)
    sigLenBytes.toString(16).padStart(64, '0') + // signature length
    padTo32ByteBoundary(sigHex);
  return `0x${calldata.slice(2, 10)}${prefix}${newTail}`;
}

/** ERC-20 approve(token → spender, exact amount) calldata. */
function approveCalldata(token, spender, amount) {
  const spenderHex = ci(spender).replace('0x', '');
  return `0x${ERC20_APPROVE_SELECTOR}000000000000000000000000${spenderHex}${BigInt(amount)
    .toString(16)
    .padStart(64, '0')}`;
}

/** max(suggested, 20 gwei) — mirror of minFeePerGas in swap-send.ts. */
function minFeePerGas(suggested) {
  return suggested > MIN_FEE_PER_GAS_WEI ? suggested : MIN_FEE_PER_GAS_WEI;
}

/** eth_feeHistory → base + tip, falling back to eth_gasPrice (swap-send.ts). */
async function fetchSuggestedFee(publicClient) {
  try {
    const feeHistory = await publicClient.request({
      method: 'eth_feeHistory',
      params: ['0x1', 'latest', [75]],
    });
    const baseFee = feeHistory.baseFeePerGas?.[1];
    const tip = feeHistory.reward?.[0]?.[0];
    if (baseFee) return BigInt(baseFee) + (tip ? BigInt(tip) : 0n);
    if (tip) return BigInt(tip);
  } catch {
    // fall through
  }
  try {
    const gasPrice = await publicClient.request({ method: 'eth_gasPrice', params: [] });
    if (gasPrice) return BigInt(gasPrice);
  } catch {
    // RPC down
  }
  return MIN_FEE_PER_GAS_WEI;
}

// ── Balances ─────────────────────────────────────────────────────────────────

async function readBalances(publicClient, address) {
  const [nativeWei, usdcRaw, eurcRaw, cirbtcRaw] = await Promise.all([
    publicClient.getBalance({ address }),
    publicClient.readContract({
      address: USDC,
      abi: ERC20_ABI,
      functionName: 'balanceOf',
      args: [address],
    }),
    publicClient.readContract({
      address: EURC,
      abi: ERC20_ABI,
      functionName: 'balanceOf',
      args: [address],
    }),
    publicClient.readContract({
      address: CIRBTC,
      abi: ERC20_ABI,
      functionName: 'balanceOf',
      args: [address],
    }),
  ]);
  return {
    nativeWei: nativeWei || 0n,
    usdcRaw: usdcRaw || 0n,
    eurcRaw: eurcRaw || 0n,
    cirbtcRaw: cirbtcRaw || 0n,
  };
}

function humanUnits(big, decimals) {
  const s = big.toString().padStart(decimals + 1, '0');
  return `${s.slice(0, -decimals)}.${s.slice(-decimals)}`;
}

function humanUsdcNative(wei) {
  return humanUnits(wei, 18);
}

// ── Permit2 signing ──────────────────────────────────────────────────────────

async function signPermit(walletClient, typedData) {
  return walletClient.signTypedData({
    domain: typedData.domain,
    types: typedData.types,
    primaryType: typedData.primaryType,
    message: typedData.message,
  });
}

// ── Evidence ─────────────────────────────────────────────────────────────────

function writeEvidence(summary) {
  const dir = resolve(import.meta.dirname, '..', '..', '..', 'output', 'qa', 'swap');
  mkdirSync(dir, { recursive: true });
  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  const file = resolve(dir, `swap-evidence-${stamp}.json`);
  writeFileSync(file, `${JSON.stringify(summary, null, 2)}\n`);
  return file;
}

// ── Main ─────────────────────────────────────────────────────────────────────

async function main() {
  console.log(`\nChakra QA swap smoke — ${MODE}`);
  console.log(`   API:      ${API_URL}`);
  console.log(`   RPC:      ${RPC_URL}`);
  console.log(
    `   Amount:   ${flags.amountIn} atomic USDC (${humanUnits(BigInt(flags.amountIn), 6)} USDC)`,
  );
  console.log(`   Slippage: ${flags.slippageBps} bps`);

  const { account } = loadSecret();
  const { publicClient, walletClient } = clients(account);
  const address = account.address;
  console.log(`   Wallet:   ${address}`);

  // 1. Funding state.
  const balances = await readBalances(publicClient, address);
  console.log(
    `   Balance:  ${humanUsdcNative(balances.nativeWei)} native USDC (gas) · ${humanUnits(balances.usdcRaw, 6)} USDC · ` +
      `${humanUnits(balances.eurcRaw, 6)} EURC · ${humanUnits(balances.cirbtcRaw, 8)} cirBTC`,
  );
  const amountIn = BigInt(flags.amountIn);
  if (amountIn <= 0n) {
    console.error('❌ --amount-in must be positive');
    process.exit(1);
  }
  const fundingOk =
    balances.usdcRaw >= amountIn && balances.nativeWei > MIN_FEE_PER_GAS_WEI * 1000n;

  // 2. Quote + canonical route enforcement.
  console.log('\n① Quote USDC → cirBTC…');
  const quote = await getQuote(USDC, CIRBTC, flags.amountIn);
  assertCanonicalRoute(quote);
  const sr = quote.sub_routes[0];
  console.log(
    `   Route:    ${sr.path.map((p) => (ci(p) === ci(USDC) ? 'USDC' : ci(p) === ci(EURC) ? 'EURC' : 'cirBTC')).join(' → ')}`,
  );
  console.log(`   Pools:    ${sr.pool_addresses.join('  ·  ')}`);
  console.log(
    `   Output:   ${sr.amount_out} atomic cirBTC (${humanUnits(BigInt(sr.amount_out), 8)} cirBTC)`,
  );
  console.log(`   Impact:   ${quote.price_impact_bps} bps ≤ 100 bps ✓`);
  console.log(`   min_out:  ${quote.minimum_output}`);

  // 3. build_tx.
  console.log('\n② build_tx…');
  const tx = await buildSwapTx(
    address,
    USDC,
    CIRBTC,
    flags.amountIn,
    quote.minimum_output,
    quote.sub_routes,
  );
  console.log(`   to:       ${tx.to}`);
  console.log(`   deadline: ${tx.deadline} (${new Date(tx.deadline * 1000).toISOString()})`);
  const approvals = Array.isArray(tx.required_approvals) ? tx.required_approvals : [];
  for (const ap of approvals) {
    console.log(
      `   approval: ${ci(ap.token) === ci(USDC) ? 'USDC' : ci(ap.token) === ci(EURC) ? 'EURC' : ap.token} → ${ap.spender} exact ${ap.amount}`,
    );
  }
  if (tx.typed_data) {
    console.log(
      `   permit2:  ${tx.typed_data.primaryType} over ${tx.typed_data.domain?.verifyingContract} (chain ${tx.typed_data.domain?.chainId})`,
    );
  } else {
    console.log('   permit2:  allowance already sufficient — no typed data');
  }

  // 4. Suggested fee (never below the 20 gwei floor).
  const suggestedFee = await fetchSuggestedFee(publicClient);
  const maxFeePerGas = minFeePerGas(suggestedFee);
  console.log(`   gas:      maxFeePerGas = ${maxFeePerGas} wei`);

  // 5. Preflight simulation of every planned transaction (eth_estimateGas).
  console.log('\n③ Preflight simulation…');
  const approvalSims = [];
  for (const ap of approvals) {
    const data = approveCalldata(ap.token, ap.spender, ap.amount);
    const gas = await publicClient.estimateGas({ account: address, to: ap.token, data });
    approvalSims.push({ token: ap.token, spender: ap.spender, amount: ap.amount, gas });
    console.log(`   approve  gas ≈ ${gas}`);
  }

  let calldata = tx.data;
  let signed = false;
  if (tx.typed_data) {
    console.log('   Signing Permit2 typed data (off-chain)…');
    const signature = await signPermit(walletClient, tx.typed_data);
    signed = signature.length > 2;
    calldata = spliceSignature(tx.data, signature);
    const spliced = calldata.length > tx.data.length;
    console.log(
      `   Permit2 signature: ${signed ? 'signed' : 'EMPTY'} · splice ${spliced ? 'applied' : 'WARNING: not applied'}`,
    );
    if (!spliced) throw new Error('Permit2 splice failed: calldata shape unexpected');
  }

  let swapGas = null;
  let swapSimError = null;
  try {
    swapGas = await publicClient.estimateGas({
      account: address,
      to: tx.to,
      data: calldata,
      value: 0n,
    });
    console.log(`   swap     gas ≈ ${swapGas} (simulation OK)`);
  } catch (err) {
    swapSimError = err.shortMessage || err.message || String(err);
    console.log(`   swap     simulation reverted: ${swapSimError}`);
  }

  // 6. Funding gate (only binding for broadcast).
  const gasNeeded = approvalSims.reduce((acc, s) => acc + s.gas, 0n) + (swapGas ?? 0n) + 100000n;
  const gasCostWei = gasNeeded * maxFeePerGas;
  const gasCostHuman = humanUsdcNative(gasCostWei);
  if (!fundingOk) {
    const missing = [];
    if (balances.usdcRaw < amountIn) {
      missing.push(
        `${humanUnits(amountIn - balances.usdcRaw, 6)} more USDC (need ${humanUnits(amountIn, 6)})`,
      );
    }
    if (balances.nativeWei < gasCostWei) {
      missing.push(
        `≈${gasCostHuman} native USDC for gas (have ${humanUsdcNative(balances.nativeWei)})`,
      );
    }
    console.warn(`\n⚠️  Funding blocker: ${missing.join('; ')}`);
    console.warn('   Stop at the successful dry-run and fund the QA wallet; do not auto-fund.');
    if (MODE === 'BROADCAST') {
      console.error('❌ Broadcast aborted before sending anything — wallet is underfunded');
      process.exit(2);
    }
  }

  if (MODE === 'DRY-RUN') {
    const verdict =
      swapSimError === null || /allowance|approve|insufficient/i.test(swapSimError)
        ? 'clean'
        : 'review';
    console.log(`\n✅ DRY-RUN complete (verdict: ${verdict}). Nothing was broadcast.`);
    console.log(
      `   Re-run with --broadcast to submit ${approvals.length} approval(s) and the swap.`,
    );
    const summary = {
      mode: 'dry-run',
      api: API_URL,
      rpc: RPC_URL,
      wallet: address,
      run_at: nowIso(),
      amount_in_atomic: flags.amountIn,
      slippage_bps: flags.slippageBps,
      route: { path: sr.path, pools: sr.pool_addresses },
      price_impact_bps: quote.price_impact_bps,
      expected_output: quote.expected_output,
      minimum_output: quote.minimum_output,
      build_tx: { to: tx.to, deadline: tx.deadline },
      approvals: approvals.map((a) => ({ token: a.token, spender: a.spender, amount: a.amount })),
      permit2_typed_data: Boolean(tx.typed_data),
      permit2_signed: signed,
      swap_simulation: swapSimError === null ? 'ok' : swapSimError,
      funding: {
        native_usdc_wei: balances.nativeWei.toString(),
        usdc_atomic: balances.usdcRaw.toString(),
        est_gas_cost_wei: gasCostWei.toString(),
        ok: Boolean(fundingOk),
      },
      verdict,
    };
    const file = writeEvidence(summary);
    console.log(`   Evidence: ${file}`);
    return;
  }

  // ── BROADCAST path ─────────────────────────────────────────────────────────
  if (swapSimError !== null && !/allowance|approve|insufficient/i.test(swapSimError)) {
    console.error(`❌ Swap simulation reverted (${swapSimError}) — aborting before broadcast`);
    process.exit(2);
  }

  console.log('\n④ Broadcasting approvals (exact amounts)…');
  const approveHashes = [];
  for (const ap of approvals) {
    const data = approveCalldata(ap.token, ap.spender, ap.amount);
    const hash = await walletClient.sendTransaction({ to: ap.token, data, maxFeePerGas });
    console.log(
      `   approve ${ci(ap.token) === ci(USDC) ? 'USDC' : ap.token} → ${ap.spender}: ${hash}`,
    );
    await publicClient.waitForTransactionReceipt({ hash, confirmations: 1 });
    approveHashes.push(hash);
  }

  console.log('\n⑤ Submitting swap…');
  let swapHash;
  try {
    swapHash = await walletClient.sendTransaction({
      to: tx.to,
      data: calldata,
      value: 0n,
      maxFeePerGas,
    });
  } catch (err) {
    console.error(`❌ Swap submission failed: ${err.shortMessage || err.message || err}`);
    console.error(`   Approvals already broadcast: ${approveHashes.join(', ')}`);
    process.exit(1);
  }
  console.log(`   tx: ${swapHash}`);
  console.log('   Waiting for receipt (1 confirmation)…');
  const receipt = await publicClient.waitForTransactionReceipt({
    hash: swapHash,
    confirmations: 1,
  });
  if (receipt.status !== 'success') {
    console.error(`❌ Swap reverted on-chain (status ${receipt.status})`);
    process.exit(1);
  }

  const after = await readBalances(publicClient, address);
  console.log('\n✅ Swap confirmed');
  console.log(`   tx hash:     ${swapHash}`);
  console.log(`   block:       ${receipt.blockNumber}`);
  console.log(`   explorer:    ${EXPLORER_URL}/tx/${swapHash}`);
  console.log(`   route:       USDC → EURC → cirBTC via ${sr.pool_addresses.join(' → ')}`);
  console.log(
    `   USDC delta:  -${humanUnits(amountIn, 6)} (${balances.usdcRaw} → ${after.usdcRaw} atomic)`,
  );
  console.log(
    `   cirBTC delta: ${humanUnits(after.cirbtcRaw - balances.cirbtcRaw, 8)} (${balances.cirbtcRaw} → ${after.cirbtcRaw} atomic)`,
  );
  console.log('   Analytics attribution: verify after 12 confirmations via /api/v1/stats.');

  const summary = {
    mode: 'broadcast',
    api: API_URL,
    rpc: RPC_URL,
    wallet: address,
    run_at: nowIso(),
    tx_hash: swapHash,
    block: receipt.blockNumber,
    explorer: `${EXPLORER_URL}/tx/${swapHash}`,
    route: { path: sr.path, pools: sr.pool_addresses },
    amount_in_atomic: flags.amountIn,
    slippage_bps: flags.slippageBps,
    price_impact_bps: quote.price_impact_bps,
    expected_output: quote.expected_output,
    minimum_output: quote.minimum_output,
    approvals_broadcast: approveHashes,
    approval_count: approveHashes.length,
    permit2_signed: signed,
    receipt_status: receipt.status,
    gas_used: receipt.gasUsed.toString(),
    balance_delta: {
      usdc_atomic: { before: balances.usdcRaw.toString(), after: after.usdcRaw.toString() },
      cirbtc_atomic: { before: balances.cirbtcRaw.toString(), after: after.cirbtcRaw.toString() },
    },
  };
  const file = writeEvidence(summary);
  console.log(`   Evidence: ${file}`);
}

main().catch((err) => {
  console.error(`\n❌ Smoke failed: ${err?.shortMessage || err?.message || err}`);
  process.exit(1);
});
