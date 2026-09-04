/**
 * A swap simulation can fail before the planned approval is sent. These
 * failures are safe to classify as approval-dependent; the broadcast path
 * submits the exact approval first and the chain enforces the final outcome.
 */
export function isApprovalDependentSimulationError(message) {
  return /allowance|approve|insufficient|transfer[_ ]from[_ ]failed/i.test(message || '');
}

export function stringifyEvidence(value) {
  return JSON.stringify(
    value,
    (_, current) => (typeof current === 'bigint' ? current.toString() : current),
    2,
  );
}
