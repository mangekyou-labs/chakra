#!/usr/bin/env node
// qa-wallet-validate.mjs — validate required env vars before Playwright starts.
// Exit code 1 = fail (browser must not launch).

const REQUIRED = [
  { key: 'QA_WALLET_SECRET', desc: 'mnemonic or private key for disposable wallet' },
  { key: 'QA_API_URL', desc: 'Chakra API base URL (e.g. https://chakra-api.onrender.com)' },
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
  console.error('\nExport these before running qa:wallet:');
  console.error('  export QA_WALLET_SECRET="your mnemonic or private key"');
  console.error('  export QA_API_URL="https://chakra-api.onrender.com"');
  process.exit(1);
}

// Scan for accidental mnemonic/password leakage in other env vars.
const SENSITIVE_KEYS = ['PRIVATE_KEY', 'MNEMONIC', 'PASSWORD', 'SECRET', 'API_KEY'];
for (const key of Object.keys(process.env)) {
  if (SENSITIVE_KEYS.some(s => key.toUpperCase().includes(s)) && key !== 'QA_WALLET_SECRET') {
    const val = process.env[key];
    if (val && val.length > 8) {
      console.warn(`⚠️  Env var ${key} contains a value (${val.slice(0, 4)}...${val.slice(-4)}). Ensure it is not logged.`);
    }
  }
}

console.log('✅ QA wallet environment validated');
console.log(`   API: ${process.env.QA_API_URL}`);
console.log(`   Chain: ${process.env.QA_CHAIN_ID || '5042002'}`);
