# DefiLlama listing (Chakra)

Chakra is listed as a **DEX Aggregator** (volume), not TVL.

## Metric

| Field | Value |
|-------|--------|
| Dashboard | [Aggregators](https://defillama.com/aggregators) · [Arc](https://defillama.com/dex-aggregators/chain/Arc) |
| Dimension | `dailyVolume` |
| Definition | **Entry notional** (`token_in` × `amount_in`), **not** hop-weighted routed volume |
| API | `GET https://api.Chakra.xyz/api/v1/stats?day=YYYY-MM-DD` |
| Aggregator | `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K` |
| Start | `2026-07-13` |

## Adapter

Ready-to-PR file: [`aggregators/Chakra/index.ts`](./aggregators/Chakra/index.ts)

Local test against upstream tree (already verified):

```text
Daily volume: ~1.02k USD (sample day)
```

## Open the PR

```bash
git clone https://github.com/DefiLlama/dimension-adapters.git
cd dimension-adapters
# fork on GitHub, then:
git remote add mine git@github.com:<you>/dimension-adapters.git
mkdir -p aggregators/Chakra
cp /path/to/Arc-dex-aggregator/integrations/defillama/aggregators/Chakra/index.ts aggregators/Chakra/
npm i
npm test aggregators Chakra
git checkout -b Chakra-aggregator-volume
git add aggregators/Chakra
git commit -m "feat: add Chakra Arc DEX aggregator volume"
git push -u mine HEAD
gh pr create --repo DefiLlama/dimension-adapters --title "Add Chakra (Arc DEX aggregator)" --body "$(cat <<'EOF'
## Summary
- Add Chakra aggregator volume adapter for Arc
- Volume = swap **entry notional** (token_in amount), not hop-weighted routed volume
- Data from https://api.Chakra.xyz/api/v1/stats (on-chain indexer)

## Links
- Website: https://Chakra.xyz
- Stats: https://Chakra.xyz/stats
- Aggregator contract: `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`

## Test
\`npm test aggregators Chakra\`

EOF
)"
```

After merge, allow up to ~24h for the dashboard.
