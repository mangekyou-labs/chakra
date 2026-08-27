# Venues — Vendored AMM cores

This directory contains vendored Uniswap AMM core contracts for local Foundry
testing and on-chain deployment. **Tests and scripts never import these directly**;
they are deployed via `deployCode` / `new` from 0.8.30 test contracts, and 0.8.30
interfaces under `src/interfaces/` are used for cross-version ABI calls.

## License

| Directory | Upstream | License |
|-----------|----------|---------|
| `uniswap-v2/` | [v2-core v1.0.1](https://github.com/Uniswap/v2-core/tree/v1.0.1) | GPL-3.0-or-later (see `LICENSE`) |
| `uniswap-v3/` | [v3-core v1.0.0](https://github.com/Uniswap/v3-core/tree/v1.0.0) | Business Source License 1.1 (see `LICENSE`) |

Our original code under `src/stable/` is Apache-2.0.

## Solc versions

- **V2**: `pragma solidity =0.5.16` — compiled with solc 0.5.16, EVM target `istanbul`.
- **V3**: `pragma solidity =0.7.6` — compiled with solc 0.7.6, EVM target `istanbul`.
- **Our code**: `pragma solidity ^0.8.30` — compiled with solc 0.8.30, EVM target `prague`.

`foundry.toml` uses `auto_detect_solc = true` with `compilation_restrictions` to
route each directory to the correct compiler. Tests/scripts must NOT import from
`venues/` directly; use `deployCode` and 0.8.30 interfaces instead.
