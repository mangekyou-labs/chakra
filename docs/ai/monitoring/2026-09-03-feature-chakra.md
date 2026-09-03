# Chakra feature monitoring

Watch for readiness pair failures (any of the six directed USDC/EURC/cirBTC
probes without a route), RPC topic-limit errors (-32012), retry storms around
-32005, and analytics lag above 100 blocks or freshness above 300 seconds for
two consecutive polls. Preserve unattributed summaries and confirm the indexer
cursor advances monotonically; quiet chains are fresh while the poller runs —
freshness is poll age, never swap age.
