# Player provider live acceptance

Execution date: 2026-07-13 JST. Representative account: `KR / Hide on bush#KR1`.

| Provider | Direct transport | Profile/rank | Matches | Champion stats | Read refresh | Result |
| --- | --- | --- | --- | --- | --- | --- |
| DeepLoL | Anonymous JSON GET, `b2c-api-cdn.deeplol.gg` | Pass; tier chart also returned using derived `last_match_id` | Pass, 20 + 20 | Pass, 76 rows | Pass; app cache/read only | Pass |
| OP.GG | Official anonymous MCP plus first-party `getGames` Flight action | Pass, 3 rank rows | Pass, 20 + 20 | Pass, 10 rows | Pass; app cache/read only | Pass |
| U.GG | Candidate GraphQL POST, `u.gg/api` | Not executable | Not executable | Not executable | Not executable | Blocked upstream |

Commands/results:

```text
cargo test -p overlay-provider-deeplol --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
DEEPLOL PLAYER LIVE OK: profile=Hide on bush first=20 second=20 champions=76

cargo test -p overlay-provider-opgg --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
OPGG PLAYER LIVE OK: profile=Hide on bush ranks=3 first=20 second=20 champions=10
```

At `2026-07-13T14:50:47Z`, a normal anonymous JSON POST to `https://u.gg/api` returned
HTTP 403, `content-type: text/html`, and `cf-mitigated: challenge` before GraphQL execution. No
challenge bypass, browser-cookie reuse, or rendered-HTML parser was attempted. U.GG's public profile
page remained visible to search crawlers, which confirms the product data exists but does not create
a stable direct JSON contract for this application.

Revalidated at `2026-07-14T00:13:09+09:00` with a minimal anonymous GraphQL health query using
`Accept: application/json`, `Content-Type: application/json`, and the normal public-site `Origin`.
The response was again HTTP 403 with `content-type: text/html`, `cf-mitigated: challenge`, and a
Cloudflare challenge-only content security policy. Public first-party search results still expose
rendered profile summaries, but endpoint discovery found no separate anonymous JSON contract. This
does not authorize using indexed HTML as provider data under the direct-JSON-only task.

The final three-provider gate is therefore **not complete** because U.GG still lacks an executable
direct JSON contract. OP.GG continuation is now verified. Site mutation is unavailable on both
implemented transports; their explicit refresh action is a cache invalidation plus forced read, as
documented in the provider-boundary ADR.
