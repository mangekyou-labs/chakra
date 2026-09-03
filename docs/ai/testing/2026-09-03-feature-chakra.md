# Chakra feature testing

Release gates, all passed:

- Rust: `cargo fmt --all -- --check`, `cargo test --workspace` (all unit + four
  api-server fixture suites green), `cargo clippy --workspace --all-targets -- -D warnings`.
- Contracts: `cd contracts/evm && forge test` — 88 passed.
- Frontend: vitest 90 passed (13 files), `tsc --noEmit`, eslint, prettier on
  touched files, `next build` (webpack production export incl. `/stats`).
- Docker: production image from rust:1.88 toolchain builds and its binaries
  embed the new error codes and stats fields.
- Live Arc worker smoke: `Arc discovery complete factories=3 pools=3` (Xylo,
  Presto, UnitFlow); `Arc WS subscribed ... batches=2` with both subscriptions
  acknowledged; no -32012/-32005 or polling failures in the observation window.
