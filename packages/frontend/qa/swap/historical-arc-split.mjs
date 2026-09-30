#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  ADDRESSES,
  BLOCK_NUMBER,
  MAX_UINT256,
  RECORDED_SPLIT,
  analyzeRollback,
  analyzeSuccess,
  buildEthCallRequest,
  decodeSimulationResult,
  validateFixture,
} from './historical-arc-split-lib.mjs';

const DEFAULT_RPC_URL = 'https://rpc.testnet.arc.io';
const scriptDir = dirname(fileURLToPath(import.meta.url));
const evmRoot = resolve(scriptDir, '../../../../contracts/evm');
const artifactPath = resolve(
  evmRoot,
  'out/HistoricalArcSplitHarness.sol/HistoricalArcSplitHarness.json',
);

function parseArgs(argv) {
  let rpcUrl = process.env.CHAKRA_RPC_HTTP || DEFAULT_RPC_URL;
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === '--rpc' && argv[index + 1]) {
      rpcUrl = argv[index + 1];
      index += 1;
    } else if (argv[index] === '--help') {
      console.log(`Usage: node qa/swap/historical-arc-split.mjs [--rpc URL]

Runs two read-only eth_call simulations at Arc block ${BLOCK_NUMBER}.
There is no private-key, transaction, or broadcast mode.`);
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${argv[index]}`);
    }
  }
  return rpcUrl;
}

function compileAndLoadHarness() {
  const build = spawnSync('forge', ['build', '--offline'], {
    cwd: evmRoot,
    encoding: 'utf8',
  });
  if (build.status !== 0) {
    process.stderr.write(build.stdout || '');
    process.stderr.write(build.stderr || '');
    throw new Error(`forge build --offline failed with exit ${build.status}`);
  }
  const artifact = JSON.parse(readFileSync(artifactPath, 'utf8'));
  const bytecode = artifact.deployedBytecode?.object;
  if (!/^0x[0-9a-fA-F]+$/.test(bytecode || '') || bytecode === '0x') {
    throw new Error(`missing harness runtime bytecode: ${artifactPath}`);
  }
  return bytecode;
}

async function rpcCall(rpcUrl, request) {
  const response = await fetch(rpcUrl, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(request),
  });
  const body = await response.json();
  if (!response.ok || body.error) {
    const detail = body.error
      ? `${body.error.code}: ${body.error.message}`
      : `HTTP ${response.status}`;
    throw new Error(`Arc RPC ${request.method} failed (${detail})`);
  }
  if (typeof body.result !== 'string') throw new Error('Arc RPC returned no eth_call result');
  return body.result;
}

function formatMicros(value) {
  const whole = value / 1_000_000n;
  const fraction = (value % 1_000_000n).toString().padStart(6, '0');
  return `${whole}.${fraction}`;
}

function printReport(success, successReport, rollbackReport) {
  console.log('Historical Arc split-swap simulation');
  console.log(`  Block:       ${BLOCK_NUMBER}`);
  console.log(`  Aggregator:  ${ADDRESSES.aggregator}`);
  console.log('  Mode:        eth_call + state override (no transaction or broadcast)');
  console.log('  Routes:');
  console.log(
    `    Presto ${formatMicros(RECORDED_SPLIT.routes[0].amountIn)} USDC -> ${ADDRESSES.presto}`,
  );
  console.log(
    `    Xylo   ${formatMicros(RECORDED_SPLIT.routes[1].amountIn)} USDC -> ${ADDRESSES.xyloPool}`,
  );
  console.log('  Success evidence:');
  console.log(`    spent=${formatMicros(successReport.spent)} USDC`);
  console.log(
    `    received=${formatMicros(successReport.received)} EURC returned=${formatMicros(success.amountOut)}`,
  );
  console.log(
    `    minimum=${formatMicros(RECORDED_SPLIT.minAmountOut)} EURC met=${successReport.meetsMinimum}`,
  );
  console.log(
    `    reserves moved: Presto=${successReport.prestoMoved} Xylo=${successReport.xyloMoved}`,
  );
  console.log(
    `      Presto path=${success.before.prestoPathReserve}->${success.after.prestoPathReserve} token=${success.before.prestoTokenReserve}->${success.after.prestoTokenReserve}`,
  );
  console.log(
    `      Xylo reserve0=${success.before.xyloReserve0}->${success.after.xyloReserve0} reserve1=${success.before.xyloReserve1}->${success.after.xyloReserve1}`,
  );
  console.log(`    aggregator catalog balances zero=${successReport.aggregatorCleared}`);
  console.log('  Rollback evidence:');
  console.log(`    SlippageExceeded=${rollbackReport.slippageExceeded}`);
  console.log(`    balances unchanged=${rollbackReport.balancesUnchanged}`);
  console.log(`    reserves unchanged=${rollbackReport.reservesUnchanged}`);
}

async function main() {
  const rpcUrl = parseArgs(process.argv.slice(2));
  validateFixture(RECORDED_SPLIT);
  const bytecode = compileAndLoadHarness();

  const success = decodeSimulationResult(
    await rpcCall(rpcUrl, buildEthCallRequest(bytecode, RECORDED_SPLIT.minAmountOut)),
  );
  const successReport = analyzeSuccess(success);

  const rollback = decodeSimulationResult(
    await rpcCall(rpcUrl, buildEthCallRequest(bytecode, MAX_UINT256)),
  );
  const rollbackReport = analyzeRollback(rollback);

  printReport(success, successReport, rollbackReport);
}

main().catch((error) => {
  console.error(`Historical Arc simulation failed: ${error.message}`);
  process.exitCode = 1;
});
