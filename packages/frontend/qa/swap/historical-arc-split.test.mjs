import assert from 'node:assert/strict';
import test from 'node:test';

import {
  ADDRESSES,
  BLOCK_NUMBER,
  HARNESS_ADDRESS,
  MAX_UINT256,
  RECORDED_SPLIT,
  analyzeRollback,
  analyzeSuccess,
  buildEthCallRequest,
  decodeSimulationResult,
  encodeSimulationResultForTest,
  validateFixture,
} from './historical-arc-split-lib.mjs';

const snapshot = (overrides = {}) => ({
  userUsdc: 21_467_000_000n,
  userEurc: 0n,
  aggregatorUsdc: 0n,
  aggregatorEurc: 0n,
  aggregatorCirbtc: 0n,
  xyloReserve0: 9_236_986_394_524n,
  xyloReserve1: 613_508_500_014n,
  prestoPathReserve: 200_000_000_000n,
  prestoTokenReserve: 200_000_000_000n,
  ...overrides,
});

test('recorded fixture pins the historical two-route allocation', () => {
  const fixture = validateFixture(RECORDED_SPLIT);

  assert.equal(fixture.amountIn, 21_467_000_000n);
  assert.equal(fixture.routes.length, 2);
  assert.deepEqual(
    fixture.routes.map((route) => route.amountIn),
    [20_516_011_900n, 950_988_100n],
  );
  assert.deepEqual(
    fixture.routes.map((route) => route.pool.toLowerCase()),
    [ADDRESSES.presto.toLowerCase(), ADDRESSES.xyloPool.toLowerCase()],
  );
  assert.equal(new Set(fixture.routes.map((route) => route.pool.toLowerCase())).size, 2);
});

test('eth_call request injects only harness code and an ephemeral native balance', () => {
  const request = buildEthCallRequest('0x60006000', RECORDED_SPLIT.minAmountOut);

  assert.equal(request.method, 'eth_call');
  assert.equal(request.params[0].to.toLowerCase(), HARNESS_ADDRESS.toLowerCase());
  assert.equal(request.params[1], `0x${BLOCK_NUMBER.toString(16)}`);
  assert.deepEqual(Object.keys(request.params[2]), [HARNESS_ADDRESS]);
  assert.equal(request.params[2][HARNESS_ADDRESS].code, '0x60006000');
  assert.equal(request.params[2][HARNESS_ADDRESS].balance, '0x48bba788af59e8c0000');
  assert.match(request.params[0].data, /^0x[0-9a-f]+$/);
});

test('successful decoded output proves spend, receipt, reserve movement, and zero aggregator balances', () => {
  const before = snapshot();
  const after = snapshot({
    userUsdc: 0n,
    userEurc: 24_900_000_000n,
    xyloReserve0: 9_237_937_382_624n,
    xyloReserve1: 612_900_000_000n,
    prestoPathReserve: 220_516_011_900n,
    prestoTokenReserve: 176_000_000_000n,
  });

  const report = analyzeSuccess({
    ok: true,
    amountOut: 24_900_000_000n,
    revertData: '0x',
    before,
    after,
  });

  assert.equal(report.received, 24_900_000_000n);
  assert.equal(report.spent, RECORDED_SPLIT.amountIn);
  assert.equal(report.meetsMinimum, true);
  assert.equal(report.xyloMoved, true);
  assert.equal(report.prestoMoved, true);
  assert.equal(report.aggregatorCleared, true);
});

test('rollback decoded output requires SlippageExceeded and unchanged balances and reserves', () => {
  const before = snapshot();
  const result = {
    ok: false,
    amountOut: 0n,
    revertData: '0x71c4efed' + '00'.repeat(64),
    before,
    after: { ...before },
  };

  const report = analyzeRollback(result);

  assert.equal(report.slippageExceeded, true);
  assert.equal(report.balancesUnchanged, true);
  assert.equal(report.reservesUnchanged, true);
});

test('ABI decoder preserves bigint snapshots and caught revert bytes', () => {
  const encoded = encodeSimulationResultForTest({
    ok: false,
    amountOut: 0n,
    revertData: '0x71c4efed',
    before: snapshot(),
    after: snapshot(),
  });
  const decoded = decodeSimulationResult(encoded);
  assert.equal(decoded.ok, false);
  assert.equal(decoded.before.userUsdc, 21_467_000_000n);
  assert.equal(decoded.revertData, '0x71c4efed');
});

test('rollback payload uses uint256.max as minimum output', () => {
  const request = buildEthCallRequest('0x60006000', MAX_UINT256);
  assert.match(request.params[0].data, /^0x[0-9a-f]+$/);
});
