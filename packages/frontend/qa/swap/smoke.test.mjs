import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const smokePath = join(dirname(fileURLToPath(import.meta.url)), 'smoke.mjs');

function envWithoutSecret() {
  const env = { ...process.env };
  delete env.QA_WALLET_SECRET;
  return env;
}

function spawnSmoke(args, { env = envWithoutSecret(), timeoutMs = 8000 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [smokePath, ...args], {
      env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`smoke.mjs timed out after ${timeoutMs}ms`));
    }, timeoutMs);
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', (err) => {
      clearTimeout(timer);
      reject(err);
    });
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal, stdout, stderr });
    });
  });
}

test('exits 1 when QA_WALLET_SECRET is missing', async () => {
  const result = await spawnSmoke([]);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /QA_WALLET_SECRET is not set/);
  assert.doesNotMatch(result.stdout, /BROADCAST/);
  assert.doesNotMatch(result.stderr, /BROADCAST/);
});

test('--help exits 0 without a wallet secret', async () => {
  const result = await spawnSmoke(['--help']);
  assert.equal(result.code, 0);
  assert.doesNotMatch(result.stderr, /QA_WALLET_SECRET is not set/);
  assert.match(result.stdout, /--broadcast/);
});

test('does not treat a CLI flag as the wallet secret', async () => {
  const result = await spawnSmoke(['--broadcast']);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /QA_WALLET_SECRET is not set/);
  assert.doesNotMatch(result.stdout, /Wallet secret:/);
  assert.doesNotMatch(result.stderr, /Wallet secret:/);
});

test('defaults to dry-run when --broadcast is absent', async () => {
  const help = await spawnSmoke(['--help']);
  assert.equal(help.code, 0);
  assert.match(help.stdout, /default is a dry run/i);
  const missing = await spawnSmoke([]);
  assert.equal(missing.code, 1);
  assert.doesNotMatch(missing.stdout, /BROADCAST/);
  assert.doesNotMatch(missing.stderr, /BROADCAST/);
});

test('rejects --amount-in without a value before reading the wallet secret', async () => {
  const result = await spawnSmoke(['--amount-in']);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /--amount-in requires a value/);
  assert.doesNotMatch(result.stderr, /QA_WALLET_SECRET is not set/);
  assert.doesNotMatch(result.stderr, /Cannot convert undefined to a BigInt/);
});
