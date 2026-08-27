#!/usr/bin/env node
// qa-wallet-setup.mjs — create gitignored persistent Chromium profile for Playwright + dAppwright.
import { execSync } from 'node:child_process';
import { mkdirSync, existsSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';

const PROFILE_DIR = resolve(import.meta.dirname, '..', 'output', 'playwright', 'chromium-profile');
const ARTIFACTS_DIR = resolve(import.meta.dirname, '..', 'output', 'playwright', 'evidence');

// Clean previous run artifacts (not the profile unless --clean flag).
const clean = process.argv.includes('--clean');
if (clean && existsSync(PROFILE_DIR)) {
  console.log('🧹 Cleaning previous profile...');
  rmSync(PROFILE_DIR, { recursive: true, force: true });
}

mkdirSync(PROFILE_DIR, { recursive: true });
mkdirSync(ARTIFACTS_DIR, { recursive: true });

// Write a manifest to the profile for traceability (no secrets).
writeFileSync(
  resolve(PROFILE_DIR, '.qa-manifest.json'),
  JSON.stringify(
    {
      created: new Date().toISOString(),
      chainId: process.env.QA_CHAIN_ID || '5042002',
      apiUrl: process.env.QA_API_URL || 'http://127.0.0.1:8080',
    },
    null,
    2,
  ),
);

console.log(`✅ Profile directory: ${PROFILE_DIR}`);
console.log(`✅ Evidence directory: ${ARTIFACTS_DIR}`);
