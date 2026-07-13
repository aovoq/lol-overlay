# Player provider live acceptance

Execution date: 2026-07-13 JST. Representative account: `KR / Hide on bush#KR1`.

Fresh revalidation: 2026-07-14 01:59 JST on macOS. The same representative account and anonymous
transports passed again. This is provider-network evidence only; it is not Windows/LCU UI evidence.

| Provider | Direct transport | Profile/rank | Matches | Champion stats | Read refresh | Result |
| --- | --- | --- | --- | --- | --- | --- |
| DeepLoL | Anonymous JSON GET, `b2c-api-cdn.deeplol.gg` | Pass; tier chart also returned using derived `last_match_id` | Pass, 20 + 20 | Pass, 76 rows | Pass; app cache/read only | Pass |
| OP.GG | Official anonymous MCP plus first-party `getGames` Flight action | Pass, 3 rank rows | Pass, 20 + 20 | Pass, 10 rows | Pass; app cache/read only | Pass |
| U.GG | Build-only `stats2.u.gg`; Player GraphQL excluded | N/A | N/A | N/A | N/A | Intentionally not registered |

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

Independently revalidated at `2026-07-14T00:20:48+09:00`. Anonymous `GET /api`, a minimal
anonymous GraphQL `POST /api`, and the public profile route all returned HTTP 403 with
`content-type: text/html`, `cf-mitigated: challenge`, and a challenge-only Cloudflare content
security policy. The GraphQL probe used only `Accept`, `Content-Type`, `Origin`, and `Referer`;
it sent no cookies or authentication. Search of U.GG's current first-party indexed pages found
profiles and product descriptions, but no separately callable anonymous player JSON contract.

The same review reran the live provider gates after remediation:

```text
cargo test -p overlay-provider-deeplol --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
DEEPLOL PLAYER LIVE OK: profile=Hide on bush first=20 second=20 champions=76

cargo test -p overlay-provider-opgg --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
OPGG PLAYER LIVE OK: profile=Hide on bush ranks=3 first=20 second=20 champions=10

cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
2 build-statistics live tests passed; this crate still has no player-stat adapter or player acceptance test.
```

The product decision on `2026-07-14` is to support DeepLoL and OP.GG as the two Player Stats
providers and keep U.GG build-only. A real-Chrome investigation confirmed the GraphQL operations
and server-rendered `window.__APOLLO_STATE__`, but match history was absent from that state and the
client GraphQL request received HTML instead of JSON. Details are in
`docs/ugg-chrome-api-investigation.md`. The final Player Stats live gate therefore requires
DeepLoL and OP.GG to pass plus a contract assertion that U.GG is not registered for Player Stats.
Site mutation is unavailable on both implemented transports; their explicit refresh action is a
cache invalidation plus forced read, as documented in the provider-boundary ADR.

## 2026-07-14 completion revalidation

```text
cargo test -p overlay-provider-deeplol --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
DEEPLOL PLAYER LIVE OK: profile=Hide on bush first=20 second=20 champions=76

cargo test -p overlay-provider-opgg --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
OPGG PLAYER LIVE OK: profile=Hide on bush ranks=3 first=20 second=20 champions=10

cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
2 passed; both are Build-statistics tests. No U.GG Player adapter or Player live test exists.
```

Production registration and Tauri/frontend/E2E contracts also assert that the Player source list is
exactly `deeplol`, `opgg`. The independent Windows/LCU manual gate is prepared in
`docs/player-windows-lcu-acceptance.md` and is explicitly not executed on this macOS host.

## 2026-07-14 final independent revalidation

Execution completed at 02:22 JST after the final correctness fixes. The same anonymous transports
and representative account passed:

```text
DEEPLOL PLAYER LIVE OK: profile=Hide on bush first=20 second=20 champions=76
OPGG PLAYER LIVE OK: profile=Hide on bush ranks=3 first=20 second=20 champions=10
U.GG Build-only ignored tests: 2 passed
```

The U.GG command ran only the crate's existing Build-statistics live tests. No U.GG Player request,
Cloudflare challenge request, cookie reuse, authentication, or bypass was attempted. The separate
Windows/LCU manual gate remains not executed.
