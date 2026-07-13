# Player provider live acceptance

Execution date: 2026-07-13 JST. Representative account: `KR / Hide on bush#KR1`.

| Provider | Direct transport | Profile/rank | Matches | Champion stats | Read refresh | Result |
| --- | --- | --- | --- | --- | --- | --- |
| DeepLoL | Anonymous JSON GET, `b2c-api-cdn.deeplol.gg` | Pass; tier chart also returned using derived `last_match_id` | Pass, 20 + 20 | Pass, 76 rows | Pass; app cache/read only | Pass |
| OP.GG | Official anonymous JSON-RPC MCP, `mcp-api.op.gg/mcp` | Pass, 3 rank rows | First 20 pass; official method has no continuation | Pass, 10 rows | Pass; app cache/read only | Partial |
| U.GG | Candidate GraphQL POST, `u.gg/api` | Not executable | Not executable | Not executable | Not executable | Blocked upstream |

Commands/results:

```text
cargo test -p overlay-provider-deeplol --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
DEEPLOL PLAYER LIVE OK: profile=Hide on bush first=20 second=20 champions=76

cargo test -p overlay-provider-opgg --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
OPGG PLAYER LIVE OK: profile=Hide on bush ranks=3 matches=20 champions=10
```

At `2026-07-13T14:50:47Z`, a normal anonymous JSON POST to `https://u.gg/api` returned
HTTP 403, `content-type: text/html`, and `cf-mitigated: challenge` before GraphQL execution. No
challenge bypass, browser-cookie reuse, or rendered-HTML parser was attempted. U.GG's public profile
page remained visible to search crawlers, which confirms the product data exists but does not create
a stable direct JSON contract for this application.

The final three-provider gate is therefore **not complete**. It requires both a direct U.GG player
JSON contract and an OP.GG continuation mechanism that can return the next 20 distinct matches.
Site mutation is unavailable on both implemented transports; their explicit refresh action is a
cache invalidation plus forced read, as documented in the provider-boundary ADR.

