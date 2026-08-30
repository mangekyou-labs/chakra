# @chakra-ag/sdk

TypeScript client for the [Chakra](https://github.com/mangekyou-labs/chakra) Arc Testnet REST API — quote, build_tx, tokens, balances, health. No wallet secrets; all signing happens in the user's wallet.

## Install

```bash
npm install @chakra-ag/sdk
```

## Quick start

```ts
import { ChakraClient } from '@chakra-ag/sdk';

const chakra = new ChakraClient({ apiUrl: 'https://api.example.com' });

const q = await chakra.quote({
  tokenIn: '0x…',
  tokenOut: '0x…',
  amountIn: '1000000',
});
console.log(q.expectedOutput);
```

## API

### `new ChakraClient({ apiUrl })`

Create a client. No authentication required for public endpoints.

### `chakra.quote(params)`

Returns the best route and expected output for a token swap.

### `chakra.buildTx(params)`

Builds an unsigned transaction XDR for the given quote.

### `chakra.listTokens()`

Returns available tokens on the Arc Testnet.

### `chakra.getBalance({ account, token })`

Returns the balance for a given account and token.

### `chakra.getHealth()`

Returns the health status of the API.

## License

Apache-2.0
