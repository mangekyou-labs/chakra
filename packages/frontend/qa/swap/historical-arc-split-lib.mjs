export const BLOCK_NUMBER = 60_763_000n;
export const HARNESS_ADDRESS = '0x000000000000000000000000000000000000c0de';
export const MAX_UINT256 = (1n << 256n) - 1n;

export const ADDRESSES = Object.freeze({
  aggregator: '0xeb12351602c56d47c4ee955193335848952b29d8',
  permit2: '0x000000000022D473030F116dDEE9F6B43aC78BA3',
  usdc: '0x3600000000000000000000000000000000000000',
  eurc: '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a',
  cirbtc: '0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF',
  presto: '0x5794a8284A29493871Fbfa3c4f343D42001424D6',
  xyloPool: '0x3DF3966F5138143dce7a9cFDdC2c0310ce083BB1',
});

export const RECORDED_SPLIT = Object.freeze({
  amountIn: 21_467_000_000n,
  minAmountOut: 24_805_280_879n,
  routes: Object.freeze([
    Object.freeze({ amountIn: 20_516_011_900n, pool: ADDRESSES.presto, dexType: 4 }),
    Object.freeze({ amountIn: 950_988_100n, pool: ADDRESSES.xyloPool, dexType: 3 }),
  ]),
});

import {
  decodeFunctionResult,
  encodeFunctionData,
  encodeFunctionResult,
  toFunctionSelector,
} from 'viem';

const snapshotComponents = [
  { name: 'userUsdc', type: 'uint256' },
  { name: 'userEurc', type: 'uint256' },
  { name: 'aggregatorUsdc', type: 'uint256' },
  { name: 'aggregatorEurc', type: 'uint256' },
  { name: 'aggregatorCirbtc', type: 'uint256' },
  { name: 'xyloReserve0', type: 'uint256' },
  { name: 'xyloReserve1', type: 'uint256' },
  { name: 'prestoPathReserve', type: 'uint256' },
  { name: 'prestoTokenReserve', type: 'uint256' },
];

export const HARNESS_ABI = [
  {
    type: 'function',
    name: 'simulate',
    stateMutability: 'nonpayable',
    inputs: [{ name: 'minAmountOut', type: 'uint256' }],
    outputs: [
      { name: 'ok', type: 'bool' },
      { name: 'amountOut', type: 'uint256' },
      { name: 'revertData', type: 'bytes' },
      { name: 'beforeState', type: 'tuple', components: snapshotComponents },
      { name: 'afterState', type: 'tuple', components: snapshotComponents },
    ],
  },
];

const SLIPPAGE_EXCEEDED_SELECTOR = toFunctionSelector('SlippageExceeded(uint256,uint256)');
const SNAPSHOT_KEYS = snapshotComponents.map(({ name }) => name);

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function snapshotEqual(left, right, keys) {
  return keys.every((key) => left[key] === right[key]);
}

export function validateFixture(fixture) {
  assert(fixture.routes.length === 2, 'recorded fixture must have exactly two routes');
  assert(
    fixture.routes.reduce((sum, route) => sum + route.amountIn, 0n) === fixture.amountIn,
    'route allocation must equal the total input',
  );
  assert(
    new Set(fixture.routes.map((route) => route.pool.toLowerCase())).size === fixture.routes.length,
    'recorded fixture routes must use distinct pools',
  );
  assert(fixture.routes[0].dexType === 4, 'first route must be Presto');
  assert(fixture.routes[1].dexType === 3, 'second route must be Xylo');
  return fixture;
}

export function buildEthCallRequest(harnessCode, minAmountOut) {
  assert(/^0x[0-9a-fA-F]+$/.test(harnessCode), 'harness bytecode must be non-empty hex');
  validateFixture(RECORDED_SPLIT);
  return {
    jsonrpc: '2.0',
    id: 1,
    method: 'eth_call',
    params: [
      {
        to: HARNESS_ADDRESS,
        data: encodeFunctionData({
          abi: HARNESS_ABI,
          functionName: 'simulate',
          args: [minAmountOut],
        }),
      },
      `0x${BLOCK_NUMBER.toString(16)}`,
      {
        [HARNESS_ADDRESS]: {
          balance: `0x${(RECORDED_SPLIT.amountIn * 10n ** 12n).toString(16)}`,
          code: harnessCode,
        },
      },
    ],
  };
}

function normalizeDecoded(decoded) {
  const [ok, amountOut, revertData, before, after] = decoded;
  return { ok, amountOut, revertData, before, after };
}

export function decodeSimulationResult(data) {
  return normalizeDecoded(
    decodeFunctionResult({ abi: HARNESS_ABI, functionName: 'simulate', data }),
  );
}

export function encodeSimulationResultForTest(result) {
  return encodeFunctionResult({
    abi: HARNESS_ABI,
    functionName: 'simulate',
    result: [result.ok, result.amountOut, result.revertData, result.before, result.after],
  });
}

export function analyzeSuccess(result) {
  assert(result.ok, `historical split reverted: ${result.revertData}`);
  const spent = result.before.userUsdc - result.after.userUsdc;
  const received = result.after.userEurc - result.before.userEurc;
  const aggregatorCleared = [
    result.after.aggregatorUsdc,
    result.after.aggregatorEurc,
    result.after.aggregatorCirbtc,
  ].every((balance) => balance === 0n);
  const xyloMoved =
    result.before.xyloReserve0 !== result.after.xyloReserve0 ||
    result.before.xyloReserve1 !== result.after.xyloReserve1;
  const prestoMoved =
    result.before.prestoPathReserve !== result.after.prestoPathReserve ||
    result.before.prestoTokenReserve !== result.after.prestoTokenReserve;
  const meetsMinimum = received >= RECORDED_SPLIT.minAmountOut;

  assert(spent === RECORDED_SPLIT.amountIn, 'the entire recorded USDC input was not spent');
  assert(received === result.amountOut, 'received EURC differs from aggregator return value');
  assert(meetsMinimum, 'received EURC is below the recorded minimum');
  assert(xyloMoved, 'Xylo reserves did not move');
  assert(prestoMoved, 'Presto reserves did not move');
  assert(aggregatorCleared, 'aggregator retained a catalog-token balance');

  return { spent, received, aggregatorCleared, xyloMoved, prestoMoved, meetsMinimum };
}

export function analyzeRollback(result) {
  assert(!result.ok, 'rollback simulation unexpectedly succeeded');
  const slippageExceeded =
    result.revertData.slice(0, 10).toLowerCase() === SLIPPAGE_EXCEEDED_SELECTOR;
  const balanceKeys = SNAPSHOT_KEYS.slice(0, 5);
  const reserveKeys = SNAPSHOT_KEYS.slice(5);
  const balancesUnchanged = snapshotEqual(result.before, result.after, balanceKeys);
  const reservesUnchanged = snapshotEqual(result.before, result.after, reserveKeys);

  assert(slippageExceeded, `expected SlippageExceeded, got ${result.revertData.slice(0, 10)}`);
  assert(balancesUnchanged, 'rollback changed user or aggregator balances');
  assert(reservesUnchanged, 'rollback changed venue reserves');

  return { slippageExceeded, balancesUnchanged, reservesUnchanged };
}
