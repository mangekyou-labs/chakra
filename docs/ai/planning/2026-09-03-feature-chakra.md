# Chakra feature plan

1. Watcher reliability: topic batching (10 + 3), merge/dedupe polling, WS
   multi-subscription with ack validation (+ native-tls for wss), and
   -32005/-32012 retry policy. — done, unit + live smoke verified.
2. Backend analytics: additive Redis indexer, strict six-probe readiness, and
   `/api/v1/stats` with honest heads/lag/freshness. — done, fixture tests green.
3. QA production smoke (viem CLI, env-only secret, dry-run default,
   `--broadcast` gate). — done.
4. Dashboard: `/stats` page, BigInt USD formatting, range in URL, loading /
   empty / error / stale-response states. — done, frontend gates green.
5. Gates: fmt, workspace tests, clippy -D warnings, forge, frontend
   tests/typecheck/lint/prettier, production build, Docker (rust:1.88),
   live Arc worker smoke (3 pools, 2 topic batches, no -32012). — all passed.
6. Two-stage rollout: backend commit → PR → merge → Render deploy/verify →
   15-minute observation → one QA-wallet USDC→cirBTC swap → analytics
   confirmation; then dashboard commit → PR → merge → Vercel deploy + Render
   recheck.
