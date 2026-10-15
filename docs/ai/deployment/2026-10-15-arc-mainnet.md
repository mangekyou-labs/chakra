# Arc mainnet cutover — 2026-10-15

## Chain and contracts

Arc mainnet uses chain ID **5042**, native USDC for gas, and
`https://rpc.mainnet.arc.io`. The operator supplied an authenticated mainnet
RPC and funded key locally; neither signing key nor Render API key belongs in
hosting environments.

| Contract | Mainnet address |
| --- | --- |
| Chakra Aggregator | `0x824af7339b4fFC04D0FD867953eCbfCc75dEAf18` |
| USDC | `0x3600000000000000000000000000000000000000` |
| EURC | `0xbEf5f6d51CB62b58e6A8f77868681825C6fe21c1` |
| cirBTC | `0x171A4217b86A807A64eB94757Db6849fb4bDbAA0` |
| Permit2 | `0x000000000022D473030F116dDEE9F6B43aC78BA3` |
| UnitFlow V3 factory | `0x5bfBCeb73d39F722B1cB83fD2F11736b28c1Be6d` |
| UnitFlow V3 original Quoter | `0x5AF6E89F0960Ff375AF84d9911D8153ef6240E34` |

USDC and EURC have 6 ERC-20 decimals; cirBTC has 8. Native USDC has
18 decimals. The Quoter uses the original five-argument V3 ABI, not QuoterV2.

Deployment transaction:
`0xd31438f148a025a4c16f5812b9d3441f231e0823671adfc087ff00b76993f1ae`,
block **26029398**. Factory allowlist transaction:
`0x15e1180bd22b4ce248825a3b16e1754d8aea6b89c17510a501a77147068ec9bd`.
Both receipts succeeded. Total gas cost: **0.072215682 native USDC**.
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
Analytics starts at deployment block 26029398. Disable automatic deployment
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
The final Render release is `dep-db84q6689jmc73c7chcg`, running commit
`eaefcb703202f8b60b2a6909ac4be4bcc385d0d7`; it became live at
2026-10-15 03:51:30 UTC. Render tracks `feature-chakra` with automatic deploys
disabled; future backend releases require an explicit deployment.

## Verification and operation

- Rust workspace tests pass.
- Frontend type checking and all 104 tests pass.
- Frontend static production build passes with webpack locally.
- Solidity suite: 88 passing tests.
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
