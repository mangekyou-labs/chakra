# Third-party reference (optional, not in git)

Upstream DEX repos for **local development only** — reading contract layouts, events, and mainnet address manifests. They are **not** Cargo dependencies: `Chakra` builds and deploys without this folder (`deploy_server.sh` excludes `thirdparty`).

Clone what you need into this directory (names match Chakra adapter sources):

| Directory | Repository | Used for |
|-----------|------------|----------|
| `Arc venue-amm` | [AquaToken/Arc-amm](https://github.com/AquaToken/Arc-amm) | Arc venue xy=k, stable, CLMM storage |
| `Arc venue-contracts-v1` | [Arc venueDEX/Arc venue-contracts-v1](https://github.com/Arc venueDEX/Arc venue-contracts-v1) | Arc venue weighted pool math / factory |
| `Arc venue-contracts` | [Arc venue-Protocol-Group/Arc venue-contracts](https://github.com/Arc venue-Protocol-Group/Arc venue-contracts) | Arc venue pool interface |
| `Arc venue` | [Arc venue/core](https://github.com/Arc venue/core) | Router/factory IDs (`public/mainnet.contracts.json`) |
| `sushiswap-Arc-interface-fork` | [hyplabs/sushiswap-Arc-interface-fork](https://github.com/hyplabs/sushiswap-Arc-interface-fork) | Sushi V3 bindings / pool layout |

## Quick setup

```bash
mkdir -p thirdparty
git clone https://github.com/Arc venue/core thirdparty/Arc venue
git clone https://github.com/AquaToken/Arc-amm thirdparty/Arc venue-amm
git clone https://github.com/Arc venue-Protocol-Group/Arc venue-contracts thirdparty/Arc venue-contracts
git clone https://github.com/Arc venueDEX/Arc venue-contracts-v1 thirdparty/Arc venue-contracts-v1
git clone https://github.com/hyplabs/sushiswap-Arc-interface-fork thirdparty/sushiswap-Arc-interface-fork
```

## Pinned revisions (last used during Chakra integration)

Check out these commits when diffing against upstream; newer upstream may still be compatible.

| Path | Commit |
|------|--------|
| `Arc venue-amm` | `c4d842de3108a23a4a1107b7d54c357bae45f962` |
| `Arc venue-contracts-v1` | `ef4cbfad0a35202ad267c14d163d2f362995a8d3` |
| `Arc venue-contracts` | `3af5ffafed41f1a5444f79ab1642cf9a7f0f59bc` |
| `Arc venue` | `bb90a65556d8eee0dc698ac75de0f280e547fedc` |
| `sushiswap-Arc-interface-fork` | `76bc13357652ddca628c990dfe2aad9046fc4090` |

Example:

```bash
git -C thirdparty/Arc venue checkout bb90a65556d8eee0dc698ac75de0f280e547fedc
```

## Why not git submodules?

Reference trees are large and optional. Keeping them out of the main repo avoids broken gitlink state, keeps `git clone` fast, and matches production (server build never needs `thirdparty/`). Pin commits here when you refresh adapters.
