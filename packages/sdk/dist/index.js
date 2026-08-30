/**
 * Chakra TypeScript SDK — quote, build_tx, tokens, balances, health.
 * No wallet secrets. Error objects carry `code` (NO_ROUTE / NOT_READY / ...).
 */
export class ChakraApiError extends Error {
    constructor(code, message) {
        super(message);
        this.name = 'ChakraApiError';
        this.code = code;
    }
}
/** Quote → build_tx steps mapping using server-owned per-hop metadata. */
export function quoteSubRoutesToSteps(subRoute) {
    return subRoute.poolAddresses.map((pool, i) => {
        const dexType = subRoute.dexTypes[i] ?? 'xyk';
        const step = {
            dex_type: dexType,
            pool_address: pool,
            token_in: subRoute.path[i] ?? '',
            token_out: subRoute.path[i + 1] ?? '',
        };
        const fee = subRoute.hopFees?.[i];
        if (fee !== undefined && fee > 0)
            step.fee_bps = fee;
        return step;
    });
}
export class ChakraClient {
    constructor(options) {
        this.baseUrl = options.apiUrl.replace(/\/$/, '');
        this.apiKey = options.apiKey;
    }
    headers(json = false) {
        const h = { Accept: 'application/json' };
        if (json)
            h['Content-Type'] = 'application/json';
        if (this.apiKey)
            h['X-API-Key'] = this.apiKey;
        return h;
    }
    async request(path, init) {
        const resp = await fetch(`${this.baseUrl}${path}`, init);
        const json = (await resp.json());
        if (!json.success) {
            const code = json.error?.code ?? 'RPC_ERROR';
            const message = json.error?.message ?? `Chakra API ${resp.status}`;
            throw new ChakraApiError(code, message);
        }
        return json.data;
    }
    async isHealthy() {
        try {
            await this.request('/api/v1/health');
            return true;
        }
        catch {
            return false;
        }
    }
    /** True when /ready returns 200 (snapshot current AND ≥1 pool key). */
    async isReady() {
        try {
            const data = (await this.request('/api/v1/ready'));
            return data?.ready === true;
        }
        catch {
            return false;
        }
    }
    async listTokens() {
        const data = (await this.request('/api/v1/tokens'));
        return (data?.tokens ?? []).map((t) => ({
            symbol: t.symbol,
            address: t.address.toLowerCase(),
            decimals: t.decimals,
        }));
    }
    async quote(params) {
        const search = new URLSearchParams({
            token_in: params.tokenIn,
            token_out: params.tokenOut,
            amount_in: params.amountIn,
        });
        if (params.slippageBps !== undefined)
            search.set('slippage_bps', String(params.slippageBps));
        if (params.maxHops !== undefined)
            search.set('max_hops', String(params.maxHops));
        if (params.maxSplits !== undefined)
            search.set('max_splits', String(params.maxSplits));
        const data = (await this.request(`/api/v1/quote?${search}`, {
            headers: this.headers(),
        }));
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
    async buildTx(params) {
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
        }));
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
    async getBalances(params) {
        const search = new URLSearchParams({ account: params.account });
        const data = (await this.request(`/api/v1/balances?${search}`, {
            headers: this.headers(),
        }));
        const erc20 = {};
        let nativeUsdc = '0';
        for (const [key, value] of Object.entries(data ?? {})) {
            if (key === 'native_usdc')
                nativeUsdc = value;
            else
                erc20[key] = value;
        }
        // API shape returned as-is: native USDC (18 dp) never summed with ERC-20.
        return { erc20, nativeUsdc };
    }
}
