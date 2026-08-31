#!/usr/bin/env node
// qa-wallet-validate.mjs — validate required env vars before Playwright starts.
// Loads the gitignored packages/frontend/.env (same file qa.wallet.config.ts loads),
// so `npm run qa:wallet:validate && npm run qa:wallet` behaves consistently.
// Exit code 1 = fail (browser must not launch).
// QA_WALLET_SECRET is never printed — only its shape (mnemonic word count / key prefix).

import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Mirrors qa.wallet.config.ts: process.loadEnvFile on packages/frontend/.env.
const ENV_FILE = resolve(import.meta.dirname, '..', '..', '.env');
if (existsSync(ENV_FILE)) {
  for (const line of readFileSync(ENV_FILE, 'utf8').split('\n')) {
    const m = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$/);
    if (m && !(m[1] in process.env)) {
      process.env[m[1]] = m[2].replace(/^["']|["']$/g, '');
    }
  }
}

const REQUIRED = [
  { key: 'QA_WALLET_SECRET', desc: 'mnemonic or private key for disposable wallet' },
  {
    key: 'QA_API_URL',
    desc: 'Chakra API base URL',
    default: 'https://chakra-api-0a5i.onrender.com',
  },
  { key: 'DAPP_URL', desc: 'Chakra UI base URL', default: 'https://chakra-ag.vercel.app' },
  { key: 'QA_CHAIN_ID', desc: 'Arc testnet chain ID (5042002)', default: '5042002' },
];

const missing = [];
for (const { key, desc, default: def } of REQUIRED) {
  const val = process.env[key] || def;
  if (!val) {
    missing.push(`  ${key} — ${desc}`);
  }
}

if (missing.length > 0) {
  console.error('❌ Missing required QA wallet environment variables:');
  console.error(missing.join('\n'));
  console.error('\nAdd QA_WALLET_SECRET to packages/frontend/.env (gitignored) or export it:');
  console.error('  export QA_WALLET_SECRET="your mnemonic or private key"');
  process.exit(1);
}

// Shape check without printing the secret itself.
const secret = process.env.QA_WALLET_SECRET.trim();
if (secret.includes(' ')) {
  const words = secret.split(/\s+/);
  console.log(`   Secret: mnemonic (${words.length} words)`);
  if (![12, 24].includes(words.length)) {
    console.error('❌ Mnemonic must be 12 or 24 words');
    process.exit(1);
  }
} else if (secret.startsWith('0x')) {
  console.log('   Secret: private key (0x-prefixed)');
} else {
  console.log('   Secret: private key (raw hex)');
}

// Scan for accidental mnemonic/password leakage in other env vars.
const SENSITIVE_KEYS = ['PRIVATE_KEY', 'MNEMONIC', 'PASSWORD', 'SECRET', 'API_KEY'];
for (const key of Object.keys(process.env)) {
  if (SENSITIVE_KEYS.some((s) => key.toUpperCase().includes(s)) && key !== 'QA_WALLET_SECRET') {
    const val = process.env[key];
    if (val && val.length > 8) {
      console.warn(
        `⚠️  Env var ${key} contains a value (${val.slice(0, 4)}...${val.slice(-4)}). Ensure it is not logged.`,
      );
    }
  }
}

console.log('✅ QA wallet environment validated');
console.log(`   API: ${process.env.QA_API_URL || 'https://chakra-api-0a5i.onrender.com'}`);
console.log(`   DApp: ${process.env.DAPP_URL || 'https://chakra-ag.vercel.app'}`);
console.log(`   Chain: ${process.env.QA_CHAIN_ID || '5042002'}`);
