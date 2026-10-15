# Arc mainnet cutover — 2026-10-15

## Chain and contracts

Arc mainnet uses chain ID **5042**, native USDC for gas, and
`https://rpc.mainnet.arc.io`. The operator supplied an authenticated mainnet
RPC and funded key locally; neither signing key nor Render API key belongs in
hosting environments.

| Contract | Mainnet address |
| --- | --- |
| Chakra Aggregator | `0xC977ABf5F9c529C39dac9306998eC130439150c0` |
| USDC | `0x3600000000000000000000000000000000000000` |
| EURC | `0xbEf5f6d51CB62b58e6A8f77868681825C6fe21c1` |
| cirBTC | `0x171A4217b86A807A64eB94757Db6849fb4bDbAA0` |
| Permit2 | `0x000000000022D473030F116dDEE9F6B43aC78BA3` |
| UnitFlow V3 factory | `0x5bfBCeb73d39F722B1cB83fD2F11736b28c1Be6d` |
| UnitFlow V3 original Quoter | `0x5AF6E89F0960Ff375AF84d9911D8153ef6240E34` |

USDC and EURC have 6 ERC-20 decimals; cirBTC has 8. Native USDC has
18 decimals. The Quoter uses the original five-argument V3 ABI, not QuoterV2.

Deployment transaction:
`0xdb821192b3ad21a22b67aee04312f149cb82b1af4fa6526fb3fbd17c0d08dccc`,
block **26037823**. Factory allowlist transaction:
`0xae59465a3cf4c41e5d080e3c6a84333623abf2232e05279d9862c957c3190f04`.
Both receipts succeeded. Total gas cost: **0.072392103 native USDC**.
The operator owns the unpaused aggregator; the UnitFlow factory is enabled as
`DexType.Clmm`. No swaps, token approvals or liquidity positions were created.

The old testnet venue addresses have no mainnet code. With the user's explicit
approval, launch uses verified UnitFlow V3 pools through the standard CLMM
adapter. Xylo, Presto, XYK and Stable configuration is empty until compatible
mainnet contracts are independently verified.

## Runtime changes

The worker loads slot0, liquidity, bitmap words and initialized ticks at one
block. Every refresh reloads tick state. Quotes may traverse empty intervals
only inside proven scan bounds; input that exceeds coverage is rejected rather
than returned as a partial fill. The UI identifies `chakra-clmm` as UnitFlow V3.

Render service: `srv-da8g4non74is73ds1jgg`,
`https://chakra-api-0a5i.onrender.com`. Use a dedicated Redis logical database
(index 1) for mainnet so testnet snapshots and analytics cannot leak across.
Analytics starts at deployment block 26037823. Disable automatic deployment
from the old branch before the cutover; deploy the validated migration commit
explicitly. Preserve the original service configuration privately for rollback.

Vercel project: `chakra-arc-dex`, production aliases
`https://chakra-ag.vercel.app` and `https://chakra-arc-dex.vercel.app`.
Set production `NEXT_PUBLIC_CHAKRA_API_URL` to the Render URL and rebuild.
Only the public API URL is needed in the frontend hosting environment.

The initial migration commit is `0c3a94e0302fcd031de0b13f679d39fe0829d696`.
Vercel production deployment `dpl_4w6CVxK7XULttwKaUNBS5iHH7kMD` was promoted
after Render passed the production quote and transaction-building checks.
Analytics namespaces normalize the aggregator address to lowercase so the
worker and API share the same cursor and totals regardless of environment casing.
The initial migration Render release was `dep-db84q6689jmc73c7chcg`, running commit
`eaefcb703202f8b60b2a6909ac4be4bcc385d0d7`; it became live at
2026-10-15 03:51:30 UTC. Render tracks `feature-chakra` with automatic deploys
disabled; future backend releases require an explicit deployment.

## Verification and operation

- Rust workspace tests pass.
- Frontend type checking and all 128 tests pass.
- Frontend static production build passes with webpack locally.
- Solidity suite after the callback fix: 91 passing tests.
- Six directed live quotes match UnitFlow's onchain Quoter at the tested
  small amounts (0.01 USDC/EURC and 10 cirBTC atomic units).
- Local Redis-backed worker/API: all six directions quote at readiness probe
  amounts; readiness returns HTTP 200; build_tx returns the deployed address
  and chain 5042 for both transaction and Permit2 domain.
- The unaudited-contract acknowledgement is scoped to mainnet, so a prior
  testnet acknowledgement does not suppress it.
- Owner, token immutables, Permit2, factory allowlist and pause state were
  checked on the deployed contract.
- Production acceptance after the final Render restart: all six directed
  USDC/EURC/cirBTC routes quote and build transactions for chain 5042 and the
  deployed aggregator; `/api/v1/ready` returns HTTP 200.
- Analytics caught up to confirmed block 26031463 (chain head 26031475),
  with zero indexing lag and a 17-second freshness age at the final check.
- Vercel CLI inspection of `chakra-ag.vercel.app` resolves to the promoted
  deployment above with status Ready and target production.

Re-run the read-only quote check with `ARC_RPC_URL` in the environment:
`cargo run -p dex-adapters --example mainnet_quote`.
Liquidity is thin in some pools; these checks do not guarantee quotes for
larger trades or prove successful mainnet swap execution. No funded swap is
part of this cutover.

`DeployMainnet.s.sol` is a one-time deployment script, not a restart command.
It requires `OPERATOR_PRIVATE_KEY` as a 0x-prefixed uint256 hex value and the
contract variables in `.env.example`. Do not rebroadcast after deployment.

For rollback, restore both the previous Render commit and its environment,
including Redis database 0, then restore the previous Vercel deployment.
Rolling back application code does not undo the mainnet contract deployment.

## Primary sources

- [Arc connection details](https://docs.arc.io/arc/references/connect-to-arc)
- [Arc contract addresses](https://docs.arc.io/arc/references/contract-addresses)
- [UnitFlow contracts](https://docs.unitflow.finance/docs/dev/contracts)
- [Render environment API](https://api-docs.render.com/reference/update-env-vars-for-service)
- [Render deploy API](https://api-docs.render.com/reference/create-deploy)

## UnitFlow execution fix — 2026-10-15

The replacement deployment above supersedes the initial aggregator
`0x824af7339b4fFC04D0FD867953eCbfCc75dEAf18`. The initial mainnet
quote/build checks did not execute a complete swap and missed two incompatibilities:
`build_tx` encoded CLMM fees in basis points instead of V3 tiers (1 bps must
encode 100), and UnitFlow pools invoke `unitFlowV3SwapCallback`, while the initial
aggregator exposed only `uniswapV3SwapCallback`. The replacement shares the
existing callback authentication between both entry points.

The frontend estimates the signed swap before opening the final wallet prompt,
sets a gas limit with 20% headroom, and checks the post-approval native USDC
balance against both swap input and maximum gas cost. Native and ERC-20 USDC
share a balance on Arc. Simulations use mainnet `eth_call` with only a Permit2
allowance override; no funded user swaps are submitted. A local Anvil fork does
not reproduce Arc native USDC precompile behavior and is unsuitable for this check.

Validation: frontend type checking and 128 tests pass; Rust workspace tests pass;
Solidity regression tests cover a full swap using the UnitFlow callback, spoofed
callers and unregistered pools. The one-basis-point fee tier is covered explicitly.

The callback release is commit `cd07beb08d008baffd5b9b22b7fedee8bb99242d`.
Render deployment: `dep-db85liom7kps73d5rb4g`; Vercel deployment:
`dpl_E3gb3X5C57n4PMbXr67uXs5Uxj2X`. The replacement is deployed at block
26037823. Deployment plus factory setup cost 0.072392103 USDC.
A read-only mainnet execution of 0.05 USDC returned 44,546 EURC atomic units,
with gas estimate 263,441 (about 0.00527 USDC at 20 gwei). No runtime-code
or user-balance override was used for that replacement-contract check.

Production acceptance after the callback release:

- Render is live at the callback release commit and `/api/v1/ready` returns 200.
- All six USDC/EURC/cirBTC directions quote and build transactions for the
  replacement aggregator and chain 5042. Encoded CLMM hop fees match V3 tiers.
  One build request timed out; its retry succeeded.
- The live API's 0.05 USDC calldata executes read-only with only the Permit2
  allowance override: output 44,548 EURC atomic, estimated gas 256,666
  (0.00513332 USDC at 20 gwei), satisfying the quoted minimum.
- Vercel CLI inspection confirms `chakra-ag.vercel.app` resolves to
  `dpl_E3gb3X5C57n4PMbXr67uXs5Uxj2X`, Ready, production.
- No funded user swap was submitted. Cancel stale wallet prompts and reload
  the application before signing for the replacement contract.

The final backend metadata correction is commit
`6b1d4a268a9f3b356f33212686b391099474510c`, Render deployment
`dep-db85ofgn74is73dtk1b0`. The API now loads the stats deployment block from
`CHAKRA_ANALYTICS_START_BLOCK`, matching the worker, instead of displaying the
initial contract's hardcoded block. API tests, including a regression for this
configured metadata, pass. The frontend release is unchanged.

The metadata correction is live. Final `/api/v1/stats` acceptance reports the
replacement aggregator, deployment block 26037823, and zero indexing lag;
`/api/v1/ready` returns HTTP 200.
