# JavaScript / TypeScript Integration

This is the smallest browser-side integration flow for a third-party app:

1. The app asks Chakra for a route.
2. Chakra builds an unsigned Arc transaction.
3. The app passes the XDR to its wallet adapter for signing.
4. The app submits the signed XDR and waits for confirmation.

Chakra does not need the user's secret key. The app only sends the user's
public address when building the transaction.

## Pure REST API example

No SDK is required. A browser app can call the public REST API directly with
`fetch`:

```ts
const API = 'https://api.Chakra.xyz';
const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';

export async function quoteAndBuild(userPublicKey: string, amountIn: string) {
  const params = new URLSearchParams({
    token_in: XLM,
    token_out: USDC,
    amount_in: amountIn,
    slippage: '0.5',
    max_hops: '3',
    max_splits: '2',
    // prefer_Arc is omitted: the default is false.
  });

  const quoteResponse = await fetch(`${API}/api/v1/quote?${params}`);
  const quoteJson = await quoteResponse.json();
  if (!quoteJson.success) throw new Error(quoteJson.error || 'Quote failed');

  const quote = quoteJson.data;
  const buildResponse = await fetch(`${API}/api/v1/build_tx`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      user_public_key: userPublicKey,
      token_in: XLM,
      token_out: USDC,
      amount_in: quote.amount_in,
      min_amount_out: quote.minimum_output,
      sub_routes: quote.sub_routes,
    }),
  });
  const buildJson = await buildResponse.json();
  if (!buildJson.success) throw new Error(buildJson.error || 'Build failed');

  return { quote, unsignedTxXdr: buildJson.data.unsigned_tx_xdr };
}
```

The returned `unsigned_tx_xdr` is passed to the app's wallet adapter for
signing. The app can then submit the signed XDR through its own Arc RPC or
through `POST /api/v1/submit_tx`.

`prefer_Arc` defaults to `false`, so the normal API returns the best route
across the supported venues. Set `prefer_Arc=1` only when the integration
specifically requires Arc-only routing; this is primarily used by the
Chakra arbitrage bot and should not be enabled by ordinary frontend swaps
without a reason.

## Optional SDK

The npm SDK is an optional TypeScript wrapper around the same REST endpoints:

```bash
npm install @Chakra/sdk
```

## Minimal browser example

The wallet is intentionally represented by a small adapter interface. The
integrator can connect it to any Arc wallet flow it already uses.

```ts
import { ChakraClient } from '@Chakra/sdk';

const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const PUBLIC_NETWORK = 'Public Global Arc Network ; September 2015';

type WalletAdapter = {
  address: string;
  signTransaction: (unsignedXdr: string, networkPassphrase: string) => Promise<string>;
};

const Chakra = new ChakraClient({
  apiUrl: 'https://api.Chakra.xyz',
  // apiKey: 'partner-key', // optional partner rate-limit key
});

export async function swapXlmToUsdc(wallet: WalletAdapter, amountIn: string) {
  // Amounts are integer strings in the token's smallest unit.
  const { quote, tx } = await Chakra.quoteAndBuild({
    tokenIn: XLM,
    tokenOut: USDC,
    amountIn,
    slippage: 0.5,       // percentage, not basis points
    // preferArc is omitted; the API default is false.
    maxHops: 3,
    maxSplits: 2,
    userPublicKey: wallet.address,
  });

  console.log('Expected output:', quote.expectedOutput);
  console.log('Route legs:', quote.subRoutes.length);
  console.log('Execution:', tx.execution);

  // Chakra returns unsigned XDR. The wallet signs it locally.
  const signedXdr = await wallet.signTransaction(
    tx.unsignedTxXdr,
    PUBLIC_NETWORK,
  );

  // Alternatively, submit signedXdr through the wallet's own Arc RPC.
  const submitted = await Chakra.submitTx({ signedTxXdr });
  const result = await Chakra.waitForTx(submitted.hash);

  if (!result.confirmed) {
    throw new Error(result.error || `Transaction ${submitted.hash} failed`);
  }

  return { hash: submitted.hash, expectedOutput: quote.expectedOutput };
}
```

## Wallet integration boundary

The wallet adapter is application-owned. It can wrap a browser extension,
mobile wallet, WalletConnect session, or a wallet service. Chakra only requires:

- the user's public Arc `G...` address;
- a signed XDR returned by the wallet;
- the same Arc network passphrase used by the API and wallet.

The wallet must not expose a secret key to the application or Chakra. For swaps
into classic-backed assets such as USDC, the user may need an asset trustline
before signing the swap.

## Signing with Arc Wallets Kit

`@creit.tech/Arc-wallets-kit` provides one interface for multiple Arc
wallets. It is independent from `@Chakra/sdk`; the REST example above can be
used with it directly.

```bash
npm install @creit.tech/Arc-wallets-kit
```

```ts
import { ArcWalletsKit } from '@creit.tech/Arc-wallets-kit';
import { walletModule } from '@creit.tech/Arc-wallets-kit/modules/wallet';
import { LobstrModule } from '@creit.tech/Arc-wallets-kit/modules/lobstr';
import { xBullModule } from '@creit.tech/Arc-wallets-kit/modules/xbull';
import { Networks } from '@creit.tech/Arc-wallets-kit/types';

ArcWalletsKit.init({
  network: Networks.PUBLIC,
  modules: [new walletModule(), new LobstrModule(), new xBullModule()],
});

export async function signWithWalletsKit(unsignedTxXdr: string) {
  const { address } = await ArcWalletsKit.authModal();
  if (!address) throw new Error('Wallet did not return a public address');

  const { signedTxXdr } = await ArcWalletsKit.signTransaction(unsignedTxXdr, {
    networkPassphrase: Networks.PUBLIC,
    address,
  });

  return { address, signedTxXdr };
}
```

After signing, submit `signedTxXdr` with the REST endpoint:

```ts
const response = await fetch('https://api.Chakra.xyz/api/v1/submit_tx', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ signed_tx_xdr: signedTxXdr }),
});

const submitted = await response.json();
if (!submitted.success) throw new Error(submitted.error || 'Submit failed');
```

## Signing with wallet directly

If the app only supports wallet, it can use wallet's API without
Wallets Kit:

```bash
npm install @Arc/wallet-api
```

```ts
import { requestAccess, signTransaction } from '@Arc/wallet-api';

export async function signWithwallet(unsignedTxXdr: string) {
  const access = await requestAccess();
  if (access.error || !access.address) {
    throw new Error(String(access.error || 'wallet did not return an address'));
  }

  const result = await signTransaction(unsignedTxXdr, {
    network: 'PUBLIC',
    address: access.address,
  });
  if (typeof result === 'string') {
    return { address: access.address, signedTxXdr: result };
  }
  if (result.error || !result.signedTxXdr) {
    throw new Error(String(result.error || 'wallet signing failed'));
  }

  return { address: access.address, signedTxXdr: result.signedTxXdr };
}
```

Other wallets can be integrated by implementing the same two responsibilities:
return the user's public address and sign the unsigned transaction XDR. The
Chakra quote and build API does not change.

## Quote and build separately

If the application needs to inspect or modify the quote before building, call
the two methods separately:

```ts
const quote = await Chakra.quote({
  tokenIn: XLM,
  tokenOut: USDC,
  amountIn: '100000000',
  slippage: 0.5,
  preferArc: true,
});

const tx = await Chakra.buildTx({
  userPublicKey: wallet.address,
  tokenIn: quote.tokenIn,
  tokenOut: quote.tokenOut,
  amountIn: quote.amountIn,
  minAmountOut: quote.minimumOutput,
  subRoutes: quote.subRoutes,
});
```

## Notes

- `amountIn`, `expectedOutput`, and `minimumOutput` are integer strings in the token's smallest unit.
- `slippage` is expressed as a percentage, for example `0.5` means 0.5%.
- `preferArc` defaults to `false`; `true` is mainly for the arbitrage bot or an explicitly Arc-only integration.
- `maxHops` and `maxSplits` are optional route-complexity controls.
- Do not cache quotes for long periods. Build and sign soon after quoting.
- Public API documentation: [OpenAPI](./openapi.yaml).
- Full integration guide: [Integrator Guide](./integrator-guide.md).
