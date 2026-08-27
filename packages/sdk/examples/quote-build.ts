/**
 * Chakra SDK example — quote USDC → EURC, print calldata + Permit2 payload.
 *
 * Run: npx tsx examples/quote-build.ts
 * Skip the live run when the API is down; unit tests are the gate.
 */
import { ChakraClient } from '../src/index';

const API_URL = process.env.API_URL || 'http://127.0.0.1:8080';
const USDC = '0x3600000000000000000000000000000000000000';
const EURC = '0x89b50855aa3be2f677cd6303cec089b5f319d72a';

async function main() {
  const client = new ChakraClient({ apiUrl: API_URL });

  const healthy = await client.isHealthy();
  if (!healthy) {
    console.log('example not executed — API not up');
    return;
  }

  const quote = await client.quote({
    tokenIn: USDC,
    tokenOut: EURC,
    amountIn: '1000000', // 1 USDC (6 dp)
    slippage: 0.5, // → slippage_bps 50
  });

  console.log('Quote:');
  console.log(JSON.stringify(quote, null, 2));

  const tx = await client.buildTx({
    user: '0x0000000000000000000000000000000000000001',
    tokenIn: USDC,
    tokenOut: EURC,
    amountIn: quote.amountIn,
    minAmountOut: quote.minimumOutput,
    subRoutes: quote.subRoutes,
  });

  console.log('build_tx:');
  console.log('to       =', tx.to);
  console.log('data     =', tx.data);
  console.log('chain_id =', tx.chainId);
  console.log('value    =', tx.value);
  console.log('deadline =', tx.deadline);
  console.log('typed_data =', JSON.stringify(tx.typedData));
  console.log('required_approvals =', JSON.stringify(tx.requiredApprovals));
}

main().catch((err) => {
  if (err instanceof Error && err.message.includes('NOT_READY')) {
    console.log('example not executed — API not ready (aggregator unconfigured)');
    return;
  }
  console.error(err);
  process.exitCode = 1;
});
