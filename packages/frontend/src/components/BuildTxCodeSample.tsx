'use client';

import { useState } from 'react';

const API_URL = process.env.NEXT_PUBLIC_CHAKRA_API_URL || 'http://localhost:8080';
const USDC = '0x3600000000000000000000000000000000000000';
const EURC = '0x89b50855aa3be2f677cd6303cec089b5f319d72a';

function buildSampleCode(): string {
  return `// Chakra quote → build_tx (no wallet secrets)
const API_URL = '${API_URL}';

// 1) Quote (slippage_bps integer, default 50 = 0.5%)
const qs = new URLSearchParams({
  token_in: '${USDC}',
  token_out: '${EURC}',
  amount_in: '1000000', // 1 USDC (6 dp)
  slippage_bps: '50',
});
const quoteResp = await fetch(\`\${API_URL}/api/v1/quote?\${qs}\`);
const quote = await quoteResp.json();
if (!quote.success) throw new Error(quote.error?.message);

// 2) Map quote → build_tx steps
// T4.7: use the server-owned per-hop dex_types[] (no source-string heuristics)
function toSteps(subRoute) {
  return subRoute.pool_addresses.map((pool, i) => ({
    dex_type: subRoute.dex_types[i], // 'xyk' | 'stable' | 'clmm' | 'xylo' | 'presto'
    pool_address: pool,
    token_in: subRoute.path[i],
    token_out: subRoute.path[i + 1],
    fee_bps: subRoute.hop_fees[i], // snapshot fee, e.g. 4 (stable) / 30 (xyk)
  }));
}

const sub_routes = quote.data.sub_routes.map((sr) => ({
  amount_in: sr.amount_in,
  steps: toSteps(sr),
}));

// 3) build_tx → to / data / typed_data (Permit2), value always "0"
const buildResp = await fetch(\`\${API_URL}/api/v1/build_tx\`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    user: '0x…[your EOA]',
    token_in: '${USDC}',
    token_out: '${EURC}',
    amount_in: quote.data.amount_in,
    min_amount_out: quote.data.minimum_output,
    sub_routes,
  }),
});
const tx = await buildResp.json();
// tx.data = { to, data, chain_id, value: "0", deadline, typed_data, required_approvals }

// 4) Sign with EIP-1193 wallet (MetaMask) and send
// docs: https://testnet.arcscan.app`;
}

export function BuildTxCodeSample() {
  const [show, setShow] = useState(false);
  const code = buildSampleCode();

  return (
    <div>
      <button type="button" onClick={() => setShow((v) => !v)}>
        {show ? 'Hide SDK sample' : 'Show SDK sample (quote → build_tx)'}
      </button>
      {show && <pre>{code}</pre>}
    </div>
  );
}
