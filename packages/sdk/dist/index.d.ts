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
export declare class ChakraApiError extends Error {
    code: string;
    constructor(code: string, message: string);
}
export interface QuoteParams {
    tokenIn: string;
    tokenOut: string;
    amountIn: string;
    /** Percent slippage (0.5 = 0.5%). Converted to `slippage_bps`. */
    slippage?: number;
    /** Integer bps (50 = 0.5%). Takes precedence over `slippage`. */
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
    /** Per-hop allowlisted factory ('' = legacy pool). T4.7. */
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
/**
 * Quote → build_tx steps mapping (T4.7). Prefers server-owned per-hop
 * `dexTypes`; falls back to `source.split(" → ")` for in-flight clients that
 * received a legacy quote. `fee_bps` is carried through so `/build_tx`
 * encodes the snapshot fee.
 */
export declare function quoteSubRoutesToSteps(subRoute: SubRoute): BuildTxStep[];
export declare class ChakraClient {
    private baseUrl;
    private apiKey?;
    constructor(options: ClientOptions & {
        apiKey?: string;
    });
    private headers;
    private request;
    isHealthy(): Promise<boolean>;
    /** True when /ready returns 200 (snapshot current AND ≥1 pool key). */
    isReady(): Promise<boolean>;
    listTokens(): Promise<TokenInfo[]>;
    quote(params: QuoteParams): Promise<QuoteResult>;
    buildTx(params: BuildTxParams): Promise<BuildTxResult>;
    getBalances(params: {
        account: string;
    }): Promise<BalancesResult>;
}
