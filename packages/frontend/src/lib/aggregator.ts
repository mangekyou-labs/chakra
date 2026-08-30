/**
 * Chakra API client for the swap UI (T6.2). Direct fetch to
 * `NEXT_PUBLIC_CHAKRA_API_URL` — no Next.js rewrite.
 */

export const CHAKRA_API_URL = process.env.NEXT_PUBLIC_CHAKRA_API_URL || 'http://localhost:8080';

export interface SubRoute {
  source: string;
  path: string[];
  pool_addresses: string[];
  /** Per-hop DEX type (`xyk` | `stable` | `clmm` | …), owned by the API. */
  dex_types?: string[];
  /** Per-hop venue fee in bps. T4.7. */
  hop_fees?: number[];
  /** Per-hop allowlisted factory; empty when the venue does not use one. */
  hop_factories?: string[];
  amount_in: string;
  amount_out: string;
  fraction_bps: number;
}

export interface QuoteData {
  amount_in: string;
  expected_output: string;
  minimum_output: string;
  price_impact_bps: number;
  protocol_fee_bps: number;
  is_split: boolean;
  max_splits: number;
  sub_routes: SubRoute[];
  compute_time_ms: number;
}

export interface QuoteResponse {
  success: boolean;
  data?: QuoteData;
  error?: { code: string; message: string };
}

export interface BuildTxStep {
  dex_type: string;
  pool_address: string;
  token_in: string;
  token_out: string;
  /** Per-hop fee in bps from the quote (T4.6/T4.7). Omit → venue default. */
  fee_bps?: number;
}

export interface BuildTxSubRoute {
  amount_in: string;
  steps: BuildTxStep[];
}

export interface BuildTxData {
  to: string;
  data: string;
  chain_id: number;
  value: string;
  deadline: number;
  typed_data: unknown | null;
  required_approvals: unknown[];
}

export interface BuildTxResponse {
  success: boolean;
  data?: BuildTxData;
  error?: { code: string; message: string };
}

/**
 * Quote → build_tx steps mapping. The API owns the per-hop DEX type and fee;
 * the client forwards those values without interpreting venue names.
 */
export function quoteSubRoutesToSteps(subRoute: SubRoute): BuildTxStep[] {
  return subRoute.pool_addresses.map((pool, i) => {
    const step: BuildTxStep = {
      dex_type: subRoute.dex_types?.[i] ?? 'xyk',
      pool_address: pool,
      token_in: subRoute.path[i] ?? '',
      token_out: subRoute.path[i + 1] ?? '',
    };
    const fee = subRoute.hop_fees?.[i];
    if (fee !== undefined && fee > 0) step.fee_bps = fee;
    return step;
  });
}

function buildTxSubRoutesFromQuote(subRoutes: SubRoute[]): BuildTxSubRoute[] {
  return subRoutes.map((subRoute) => ({
    amount_in: subRoute.amount_in,
    steps: quoteSubRoutesToSteps(subRoute),
  }));
}

function envelopeError(json: { error?: { code?: string; message?: string } }): string {
  return json.error?.message || 'Request failed';
}

export async function getQuote(
  tokenIn: string,
  tokenOut: string,
  amountIn: string,
  opts?: { slippageBps?: number; signal?: AbortSignal },
): Promise<QuoteResponse> {
  const params = new URLSearchParams({
    token_in: tokenIn,
    token_out: tokenOut,
    amount_in: amountIn,
  });
  if (opts?.slippageBps !== undefined) {
    params.set('slippage_bps', String(opts.slippageBps));
  }

  const resp = await fetch(`${CHAKRA_API_URL}/api/v1/quote?${params}`, {
    signal: opts?.signal,
  });
  const json = (await resp.json()) as QuoteResponse & {
    error?: { code?: string; message?: string };
  };
  if (!json.success || !json.data) {
    return { success: false, error: { code: 'NO_ROUTE', message: envelopeError(json) } };
  }
  return json;
}

export async function buildSwapTx(
  user: string,
  tokenIn: string,
  tokenOut: string,
  amountIn: string,
  minAmountOut: string,
  subRoutes: SubRoute[],
): Promise<BuildTxResponse> {
  const resp = await fetch(`${CHAKRA_API_URL}/api/v1/build_tx`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user,
      token_in: tokenIn,
      token_out: tokenOut,
      amount_in: amountIn,
      min_amount_out: minAmountOut,
      sub_routes: buildTxSubRoutesFromQuote(subRoutes),
    }),
  });
  const json = (await resp.json()) as BuildTxResponse;
  if (!json.success || !json.data) {
    return {
      success: false,
      error: { code: json.error?.code ?? 'ROUTE_INVALID', message: envelopeError(json) },
    };
  }
  return json;
}
