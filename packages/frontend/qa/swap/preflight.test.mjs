import test from 'node:test';
import assert from 'node:assert/strict';
import { isApprovalDependentSimulationError, stringifyEvidence } from './preflight.mjs';

test('treats transfer-from failure as expected before the planned approval', () => {
  assert.equal(
    isApprovalDependentSimulationError('Execution reverted with reason: TRANSFER_FROM_FAILED'),
    true,
  );
});

test('does not hide unrelated simulation failures', () => {
  assert.equal(isApprovalDependentSimulationError('Execution reverted: INVALID_ROUTE'), false);
});

test('serializes BigInt evidence values as strings', () => {
  assert.equal(
    stringifyEvidence({ blockNumber: 60438104n, nested: [1n] }),
    '{\n  "blockNumber": "60438104",\n  "nested": [\n    "1"\n  ]\n}',
  );
});
