import { afterEach, describe, expect, it, vi } from 'vitest';
import { ChakraClient, quoteSubRoutesToSteps } from './index';

const API = 'http://127.0.0.1:8080';
const USDC = '0x3600000000000000000000000000000000000000';
const EURC = '0x89b50855aa3be2f677cd6303cec089b5f319d72a';

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubFetch(handler: (url: string, init?: RequestInit) => unknown) {
  vi.stubGlobal('fetch', vi.fn(async (url: string, init?: RequestInit) => {
    const body = handler(url, init);
    return {
      ok: true,
      status: 200,
      json: async () => body,
    } as Response;
  }));
}

describe('ChakraClient.quote', () => {
  it('encodes token_in/token_out/amount_in/slippage_bps and never sends prefer_arc or percent slippage', async () => {
    stubFetch((url) => {
      const parsed = new URL(url);
      expect(parsed.searchParams.get('token_in')).toBe(USDC);
      expect(parsed.searchParams.get('token_out')).toBe(EURC);
      expect(parsed.searchParams.get('amount_in')).toBe('1000000');
      expect(parsed.searchParams.get('slippage_bps')).toBe('50');
      expect(parsed.searchParams.has('prefer_arc')).toBe(false);
      expect(parsed.searchParams.has('slippage')).toBe(false);
      return {
        success: true,
        data: {
          amount_in: '1000000',
          expected_output: '999550',
          minimum_output: '994552',
          price_impact_bps: 4,
          protocol_fee_bps: 0,
          is_split: false,
          max_splits: 5,
          sub_routes: [],
          compute_time_ms: 2,
        },
        error: null,
      };
    });

    const client = new ChakraClient({ apiUrl: API });
    await client.quote({ tokenIn: USDC, tokenOut: EURC, amountIn: '1000000', slippage: 0.5 });
  });

  it('parses price_impact_bps, protocol_fee_bps, is_split, fraction_bps, sub_routes', async () => {
    stubFetch(() => ({
      success: true,
      data: {
        amount_in: '1000000',
        expected_output: '999550',
        minimum_output: '994552',
        price_impact_bps: 12,
        protocol_fee_bps: 0,
        is_split: true,
        max_splits: 5,
        sub_routes: [
          {
            source: 'chakra-xyk → chakra-clmm',
            path: [USDC, '0x3333333333333333333333333333333333333333', EURC],
            pool_addresses: ['0x1111111111111111111111111111111111111111', '0x2222222222222222222222222222222222222222'],
            dex_types: ['xyk', 'clmm'],
            hop_fees: [30, 30],
            hop_factories: ['0xffffffffffffffffffffffffffffffffffffffff', ''],
            amount_in: '700000',
            amount_out: '600000',
            fraction_bps: 7000,
          },
        ],
        compute_time_ms: 2,
      },
      error: null,
    }));

    const client = new ChakraClient({ apiUrl: API });
    const quote = await client.quote({ tokenIn: USDC, tokenOut: EURC, amountIn: '1000000', slippage: 0.5 });
    expect(quote.priceImpactBps).toBe(12);
    expect(quote.protocolFeeBps).toBe(0);
    expect(quote.isSplit).toBe(true);
    expect(quote.subRoutes[0].fractionBps).toBe(7000);
    expect(quote.subRoutes[0].source).toBe('chakra-xyk → chakra-clmm');
    // T4.7: server-owned per-hop metadata is parsed, not reconstructed.
    expect(quote.subRoutes[0].dexTypes).toEqual(['xyk', 'clmm']);
    expect(quote.subRoutes[0].hopFees).toEqual([30, 30]);
    expect(quote.subRoutes[0].hopFactories).toEqual(['0xffffffffffffffffffffffffffffffffffffffff', '']);
  });
});

describe('ChakraClient.buildTx', () => {
  it('POSTs user (not from/user_public_key), token_in, amount_in, min_amount_out, sub_routes[].steps', async () => {
    let captured: RequestInit | null = null;
    stubFetch((_url, init) => {
      captured = init ?? null;
      return {
        success: true,
        data: {
          to: '0x00000000000000000000000000000000000000aa',
          data: '0x2e3be0c1',
          chain_id: 5042002,
          value: '0',
          deadline: 1780000000,
          typed_data: null,
          required_approvals: [],
        },
        error: null,
      };
    });

    const client = new ChakraClient({ apiUrl: API });
    await client.buildTx({
      user: '0x1234567890123456789012345678901234567890',
      tokenIn: USDC,
      tokenOut: EURC,
      amountIn: '1000000',
      minAmountOut: '994552',
      subRoutes: [
        {
          source: 'chakra-stable',
          path: [USDC, EURC],
          poolAddresses: ['0x0000000000000000000000000000000000000002'],
          dexTypes: ['stable'],
          hopFees: [4],
          hopFactories: [''],
          amountIn: '1000000',
          amountOut: '999550',
          fractionBps: 10000,
        },
      ],
    });

    const body = JSON.parse(String(captured?.body));
    expect(body.user).toBe('0x1234567890123456789012345678901234567890');
    expect(body.user_public_key).toBeUndefined();
    expect(body.from).toBeUndefined();
    expect(body.token_in).toBe(USDC);
    expect(body.amount_in).toBe('1000000');
    expect(body.min_amount_out).toBe('994552');
    expect(body.sub_routes[0].steps).toEqual([
      {
        dex_type: 'stable',
        pool_address: '0x0000000000000000000000000000000000000002',
        token_in: USDC,
        token_out: EURC,
        fee_bps: 4,
      },
    ]);
  });
});

describe('quoteSubRoutesToSteps', () => {
  it('maps a two-hop chakra-xyk → chakra-clmm source into xyk then clmm steps', () => {
    const steps = quoteSubRoutesToSteps({
      source: 'chakra-xyk → chakra-clmm',
      path: [USDC, '0x3333333333333333333333333333333333333333', EURC],
      poolAddresses: ['0x1111111111111111111111111111111111111111', '0x2222222222222222222222222222222222222222'],
      dexTypes: ['xyk', 'clmm'],
      hopFees: [30, 30],
      hopFactories: ['', ''],
      amountIn: '700000',
      amountOut: '600000',
      fractionBps: 7000,
    });
    expect(steps).toEqual([
      { dex_type: 'xyk', pool_address: '0x1111111111111111111111111111111111111111', token_in: USDC, token_out: '0x3333333333333333333333333333333333333333', fee_bps: 30 },
      { dex_type: 'clmm', pool_address: '0x2222222222222222222222222222222222222222', token_in: '0x3333333333333333333333333333333333333333', token_out: EURC, fee_bps: 30 },
    ]);
  });

  it('falls back to the joined source when dexTypes is absent (legacy quote)', () => {
    const steps = quoteSubRoutesToSteps({
      source: 'chakra-stable',
      path: [USDC, EURC],
      poolAddresses: ['0x0000000000000000000000000000000000000002'],
      dexTypes: [],
      hopFees: [],
      hopFactories: [],
      amountIn: '1000000',
      amountOut: '999550',
      fractionBps: 10000,
    });
    expect(steps).toEqual([
      { dex_type: 'stable', pool_address: '0x0000000000000000000000000000000000000002', token_in: USDC, token_out: EURC },
    ]);
  });

  it('uses server dex_types over the joined source when they disagree', () => {
    const steps = quoteSubRoutesToSteps({
      source: 'chakra-xyk',
      path: [USDC, EURC],
      poolAddresses: ['0x0000000000000000000000000000000000000002'],
      dexTypes: ['stable'],
      hopFees: [4],
      hopFactories: [''],
      amountIn: '1000000',
      amountOut: '999550',
      fractionBps: 10000,
    });
    expect(steps[0].dex_type).toBe('stable');
    expect(steps[0].fee_bps).toBe(4);
  });

  it('maps xylo-stable and xylo sources to xylo dex_type when dexTypes is absent', () => {
    const steps = quoteSubRoutesToSteps({
      source: 'xylo-stable',
      path: [USDC, EURC],
      poolAddresses: ['0x3DF3966F5138143dce7a9cFDdC2c0310ce083BB1'],
      dexTypes: [],
      hopFees: [4],
      hopFactories: ['0x60EDeFB094B84BBC6430cc130B358A43Ba1979e2'],
      amountIn: '1000000',
      amountOut: '865542',
      fractionBps: 10000,
    });
    expect(steps[0].dex_type).toBe('xylo');
    expect(steps[0].fee_bps).toBe(4);
  });

  it('maps presto-hub and presto sources to presto dex_type when dexTypes is absent', () => {
    const steps = quoteSubRoutesToSteps({
      source: 'presto-hub',
      path: [USDC, EURC],
      poolAddresses: ['0x5794a8284A29493871Fbfa3c4f343D42001424D6'],
      dexTypes: [],
      hopFees: [30],
      hopFactories: ['0x5794a8284A29493871Fbfa3c4f343D42001424D6'],
      amountIn: '1000000',
      amountOut: '996915',
      fractionBps: 10000,
    });
    expect(steps[0].dex_type).toBe('presto');
    expect(steps[0].fee_bps).toBe(30);
  });

  it('maps unitflow-v25 source to xyk dex_type when dexTypes is absent', () => {
    const steps = quoteSubRoutesToSteps({
      source: 'unitflow-v25',
      path: [EURC, '0xf0c4a4ce82a5746abaad9425360ab04fbba432bf'],
      poolAddresses: ['0x268DC75517EaFc6e0D52666639529e5DAB8c9200'],
      dexTypes: [],
      hopFees: [30],
      hopFactories: ['0xd67F63A4F26a497b364d1C82e6747Aec8B5743a5'],
      amountIn: '1000000',
      amountOut: '240000',
      fractionBps: 10000,
    });
    expect(steps[0].dex_type).toBe('xyk');
    expect(steps[0].fee_bps).toBe(30);
  });
});

describe('envelope errors', () => {
  it('throws an error whose .code is NO_ROUTE / NOT_READY / PAUSED', async () => {
    stubFetch(() => ({
      success: false,
      data: null,
      error: { code: 'NO_ROUTE', message: 'No route available for this pair' },
    }));

    const client = new ChakraClient({ apiUrl: API });
    await expect(
      client.quote({ tokenIn: USDC, tokenOut: EURC, amountIn: '1000000' }),
    ).rejects.toMatchObject({ code: 'NO_ROUTE' });
  });
});

describe('isHealthy', () => {
  it('uses /api/v1/health', async () => {
    stubFetch((url) => {
      expect(url).toBe(`${API}/api/v1/health`);
      return { success: true, data: { status: 'ok' }, error: null };
    });
    const client = new ChakraClient({ apiUrl: API });
    await expect(client.isHealthy()).resolves.toBe(true);
  });
});
