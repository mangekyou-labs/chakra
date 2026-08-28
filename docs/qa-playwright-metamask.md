# MetaMask E2E QA Harness (Playwright + dAppwright)

This document describes the automated end-to-end testing harness for Chakra on Arc Testnet using real MetaMask extension automation.

## Architecture

The harness uses `@tenkeylabs/dappwright` on top of `@playwright/test` to automate a real headed Chromium browser with the official MetaMask extension loaded.

- **Network:** Arc Testnet (Chain ID `5042002` / `0x4CEF52`)
- **RPC URL:** `https://rpc.testnet.arc.io` (public Arc RPC)
- **Currency Symbol:** `USDC` (native 18 dp gas token)
- **Catalog Tokens:**
  - ERC-20 USDC: `0x3600000000000000000000000000000000000000` (6 dp)
  - EURC: `0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a` (6 dp)

## Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `QA_WALLET_SECRET` | 12/24-word seed phrase or `0x`-prefixed private key | `""` | Yes (for live test) |
| `QA_API_URL` | Base URL of the Chakra REST API | `https://chakra-api-0a5i.onrender.com` | No |
| `DAPP_URL` | Base URL of the Chakra Frontend UI | `https://chakra-arc-dex.vercel.app` | No |
| `QA_CHAIN_ID` | Arc testnet chain ID | `5042002` | No |

## Setup & Validation

From `packages/frontend/`:

1. **Pre-cache MetaMask extension & create profile directory:**
   ```bash
   npm run qa:wallet:setup
   ```

2. **Validate environment configuration:**
   ```bash
   npm run qa:wallet:validate
   ```

3. **Run the critical-path test:**
   ```bash
   npm run qa:wallet
   ```

## Test Flow (Critical Path)

1. **Bootstrap:** Launches headed Chromium, initializes MetaMask with `QA_WALLET_SECRET`.
2. **Network Setup:** Adds and switches to Arc Testnet (`5042002`).
3. **Connect:** Navigates to `DAPP_URL`, clicks Connect, selects MetaMask, approves in extension.
4. **Quote:** Enters swap size (e.g. `1.0` USDC -> EURC), verifies routes and 0.00% protocol fee.
5. **Permit2 Approval:** Approves ERC-20 Permit2 allowance if required.
6. **Swap Execution:** Clicks Swap, signs EIP-712 PermitSingle typed data, confirms `splitSwap` transaction (`value = 0n`).
7. **Verification:** Waits for 1 confirmation, verifies Arcscan link and `localStorage` recent swaps.
8. **Skip Safety:** If `QA_WALLET_SECRET` is unset, the spec automatically skips without failing CI.

## Security & Artifact Sanitization

- **No Secrets in CLI / Logs:** `QA_WALLET_SECRET` is read solely from environment variables.
- **Gitignored Output:** All browser profiles, traces, screenshots, and reports are saved under `output/playwright/` (gitignored).
- **Sanitization:** Artifacts captured during runs are stripped of credential data.
