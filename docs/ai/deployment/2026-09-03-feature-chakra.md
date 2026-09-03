# Chakra feature deployment

Two-stage release from the feature-chakra branch. Stage 1 ships the Rust
watcher/API, render.yaml, QA tooling, and backend lifecycle docs; stage 2 ships
the `/stats` dashboard and final documentation. Render worker/API use the
existing Arc targets and additive `chakra:analytics:*` namespace; the frontend
deploys to the linked chakra-arc-dex Vercel project. Rollback reference for the
prior backend: `dep-daagnntg1s2s73d4rh70`.
