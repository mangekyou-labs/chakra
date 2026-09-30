# Grant Testnet evidence

`stats-2026-09-30.json` is the unmodified public response fetched on 30 September 2026 from `https://chakra-api-0a5i.onrender.com/api/v1/stats?range=all` (HTTP 200).

The API reports five confirmed swaps, 5 USDC notional, two unique addresses and zero split swaps. `freshness_secs` is 157662 (about 44 hours); this is an indexed Testnet snapshot, not a real-time activity or customer-acquisition claim.

The existing 5 September 2026 external-wallet receipt is [on Arcscan](https://testnet.arcscan.app/tx/0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c). Earlier testing records describe a successful 1 USDC → 1.629188 EURC Presto single-route swap. The explorer API returned HTTP 403 during this refresh, so those receipt details are recovered historical evidence rather than a fresh chain check.

[Recorded demo supplied by the founder](https://youtu.be/12hvXfd5rsE). Playback, duration and product claims still need a submission check.
