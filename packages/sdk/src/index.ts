/**
 * Chakra TypeScript SDK — quote, build_tx, tokens, balances, health.
 * No wallet secrets. Error objects carry `code` (NO_ROUTE / NOT_READY / ...).
 */

export interface ClientOptions {
  apiUrl: string;
}

export interface EnvelopeError {
  code: string;
  message: string;
}

export class ChakraApiError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = 'ChakraApiError';
    this.code = code;
  }
}

export interface QuoteParams {
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  /** Integer basis points (50 = 0.5%). */
  slippageBps?: number;
  maxHops?: number;
  maxSplits?: number;
}

export interface SubRoute {
  source: string;
  path: string[];
  poolAddresses: string[];
  /** Per-hop DEX type (`xyk` | `stable` | `clmm` | …). T4.7 — server-owned. */
  dexTypes: string[];
  /** Per-hop venue fee in bps. T4.7. */
  hopFees: number[];
  /** Per-hop allowlisted factory; empty when the venue does not use one. */
  hopFactories: string[];
  amountIn: string;
  amountOut: string;
  fractionBps: number;
}

export interface QuoteResult {
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  expectedOutput: string;
  minimumOutput: string;
  priceImpactBps: number;
  protocolFeeBps: number;
  isSplit: boolean;
  maxSplits: number;
  subRoutes: SubRoute[];
  computeTimeMs: number;
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

export interface BuildTxParams {
  user: string;
  tokenIn: string;
  tokenOut: string;
  amountIn: string;
  minAmountOut: string;
  subRoutes: SubRoute[];
}

export interface BuildTxResult {
  to: string;
  data: string;
  chainId: number;
  value: string;
  deadline: number;
  typedData: unknown | null;
  requiredApprovals: unknown[];
}

export interface TokenInfo {
  symbol: string;
  address: string;
  decimals: number;
}

export interface BalancesResult {
  /** ERC-20 balances keyed by symbol (usdc/eurc/cirbtc). */
  erc20: Record<string, string>;
  /** Native USDC (18 dp) — gas only, never summed with ERC-20. */
  nativeUsdc: string;
}

export interface TokenRow {
  symbol: string;
  address: string;
  decimals: number;
}

/** Quote → build_tx steps mapping using server-owned per-hop metadata. */
export function quoteSubRoutesToSteps(subRoute: SubRoute): BuildTxStep[] {
  return subRoute.poolAddresses.map((pool, i) => {
    const dexType = subRoute.dexTypes[i] ?? 'xyk';
    const step: BuildTxStep = {
      dex_type: dexType,
      pool_address: pool,
      token_in: subRoute.path[i] ?? '',
      token_out: subRoute.path[i + 1] ?? '',
    };
    const fee = subRoute.hopFees?.[i];
    if (fee !== undefined && fee > 0) step.fee_bps = fee;
    return step;
  });
}

export class ChakraClient {
  private baseUrl: string;
  private apiKey?: string;

  constructor(options: ClientOptions & { apiKey?: string }) {
    this.baseUrl = options.apiUrl.replace(/\/$/, '');
    this.apiKey = options.apiKey;
  }

  private headers(json = false): Record<string, string> {
    const h: Record<string, string> = { Accept: 'application/json' };
    if (json) h['Content-Type'] = 'application/json';
    if (this.apiKey) h['X-API-Key'] = this.apiKey;
    return h;
  }

  private async request(path: string, init?: RequestInit): Promise<unknown> {
    const resp = await fetch(`${this.baseUrl}${path}`, init);
    const json = (await resp.json()) as {
      success?: boolean;
      data?: unknown;
      error?: { code?: string; message?: string };
    };
    if (!json.success) {
      const code = json.error?.code ?? 'RPC_ERROR';
      const message = json.error?.message ?? `Chakra API ${resp.status}`;
      throw new ChakraApiError(code, message);
    }
    return json.data;
  }

  async isHealthy(): Promise<boolean> {
    try {
      await this.request('/api/v1/health');
      return true;
    } catch {
      return false;
    }
  }

  /** True when /ready returns 200 (snapshot current AND ≥1 pool key). */
  async isReady(): Promise<boolean> {
    try {
      const data = (await this.request('/api/v1/ready')) as { ready?: boolean };
      return data?.ready === true;
    } catch {
      return false;
    }
  }

  async listTokens(): Promise<TokenInfo[]> {
    const data = (await this.request('/api/v1/tokens')) as { tokens?: TokenRow[] };
    return (data?.tokens ?? []).map((t) => ({
      symbol: t.symbol,
      address: t.address.toLowerCase(),
      decimals: t.decimals,
    }));
  }

  async quote(params: QuoteParams): Promise<QuoteResult> {
    const search = new URLSearchParams({
      token_in: params.tokenIn,
      token_out: params.tokenOut,
      amount_in: params.amountIn,
    });
    if (params.slippageBps !== undefined) search.set('slippage_bps', String(params.slippageBps));
    if (params.maxHops !== undefined) search.set('max_hops', String(params.maxHops));
    if (params.maxSplits !== undefined) search.set('max_splits', String(params.maxSplits));

    const data = (await this.request(`/api/v1/quote?${search}`, {
      headers: this.headers(),
    })) as {
      amount_in: string;
      expected_output: string;
      minimum_output: string;
      price_impact_bps: number;
      protocol_fee_bps: number;
      is_split: boolean;
      max_splits: number;
      sub_routes: Array<{
        source: string;
        path: string[];
        pool_addresses: string[];
        dex_types?: string[];
        hop_fees?: number[];
        hop_factories?: string[];
        amount_in: string;
        amount_out: string;
        fraction_bps: number;
      }>;
      compute_time_ms: number;
    };
    return {
      tokenIn: params.tokenIn,
      tokenOut: params.tokenOut,
      amountIn: data.amount_in,
      expectedOutput: data.expected_output,
      minimumOutput: data.minimum_output,
      priceImpactBps: data.price_impact_bps,
      protocolFeeBps: data.protocol_fee_bps,
      isSplit: data.is_split,
      maxSplits: data.max_splits,
      subRoutes: (data.sub_routes ?? []).map((sr) => ({
        source: sr.source,
        path: sr.path,
        poolAddresses: sr.pool_addresses,
        dexTypes: sr.dex_types ?? [],
        hopFees: sr.hop_fees ?? [],
        hopFactories: sr.hop_factories ?? [],
        amountIn: sr.amount_in,
        amountOut: sr.amount_out,
        fractionBps: sr.fraction_bps,
      })),
      computeTimeMs: data.compute_time_ms,
    };
  }

  async buildTx(params: BuildTxParams): Promise<BuildTxResult> {
    const body = {
      user: params.user,
      token_in: params.tokenIn,
      token_out: params.tokenOut,
      amount_in: params.amountIn,
      min_amount_out: params.minAmountOut,
      sub_routes: params.subRoutes.map((sr) => ({
        amount_in: sr.amountIn,
        steps: quoteSubRoutesToSteps(sr),
      })),
    };
    const data = (await this.request('/api/v1/build_tx', {
      method: 'POST',
      headers: this.headers(true),
      body: JSON.stringify(body),
    })) as {
      to: string;
      data: string;
      chain_id: number;
      value: string;
      deadline: number;
      typed_data: unknown | null;
      required_approvals: unknown[];
    };
    return {
      to: data.to,
      data: data.data,
      chainId: data.chain_id,
      value: data.value,
      deadline: data.deadline,
      typedData: data.typed_data,
      requiredApprovals: data.required_approvals ?? [],
    };
  }

  async getBalances(params: { account: string }): Promise<BalancesResult> {
    const search = new URLSearchParams({ account: params.account });
    const data = (await this.request(`/api/v1/balances?${search}`, {
      headers: this.headers(),
    })) as Record<string, string>;
    const erc20: Record<string, string> = {};
    let nativeUsdc = '0';
    for (const [key, value] of Object.entries(data ?? {})) {
      if (key === 'native_usdc') nativeUsdc = value;
      else erc20[key] = value;
    }
    // API shape returned as-is: native USDC (18 dp) never summed with ERC-20.
    return { erc20, nativeUsdc };
  }
}
