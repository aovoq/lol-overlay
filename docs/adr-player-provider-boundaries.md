# ADR: player-provider upstream boundaries

Status: accepted, 2026-07-13; amended 2026-07-17.

## DeepLoL realtime rank

`summoner-realtime` requires a `summoner_id` query parameter, but accepts an empty value when a
valid `puu_id` is also supplied. Some valid basic profiles return that empty value. The adapter
therefore always sends both fields, preserving the resolver's `summoner_id` verbatim, so those
profiles still receive current Solo/Flex rank. It does not infer the ID or mix LCU/another
provider's rank.

## DeepLoL refresh

Read freshness and site mutation are separate capabilities. DeepLoL refresh first checks
`match/check-refresh`; only an `available` response permits the tier, match, and champion-stat
mutation sequence on `renew.deeplol.gg`. Server-provided delays and the official client's 45-second
cooldown are both enforced, and concurrent refreshes are serialized before any mutation. A
successful mutation invalidates the app cache and forces fresh anonymous reads.

## OP.GG refresh

The official MCP has no refresh tool, but OP.GG's public profile application exposes first-party
React server actions named `renewalStatus` and `renewal`. They use the same anonymous structured
Flight transport already accepted for match pagination. The adapter discovers their deployment
identifiers from the current public bundles instead of pinning hashes.

Before mutation, the adapter reads `renewableAt` and returns a typed rate-limit error while the
server cooldown is active. Allowed renewals are serialized, issued once, and polled only at the
server-provided delay until `RENEWAL_FINISH`. The returned cooldown and a 60-second local minimum
are both retained so repeated clicks and concurrent requests cannot multiply mutations. Failure or
unknown statuses fail closed; no other provider is queried.

## OP.GG pagination

The official MCP match-history method returns at most 20 matches and exposes no continuation token.
The public OP.GG app, however, uses a first-party React server action named `getGames`. Its
structured Flight result contains 20 complete games, participant details, and
`meta.last_game_created_at`; the app sends that timestamp back as `endedAt` for the next page.

The adapter keeps profile/rank/champion aggregation on the official MCP and uses this anonymous,
read-only server action for match pages. It discovers the current action identifier from the
JavaScript bundles referenced by the public profile page and caches it, rather than pinning a
deployment hash. It does not parse rendered labels or DOM structure, and it requires no browser
cookies. If the action or structured result disappears, the adapter fails with `InvalidData`
instead of replaying page one or synthesizing a cursor.

## U.GG player provider

U.GG is intentionally build-only. The observable player GraphQL endpoint is intercepted by a
managed Cloudflare challenge for anonymous direct clients. A real-Chrome investigation found
profile, rank, historic-rank, and champion aggregates in `window.__APOLLO_STATE__`, but no match
history; the client match query received HTML instead of JSON. The project will not automate
Turnstile, borrow user cookies, or evade bot protection. Consequently U.GG player support remains
unregistered and is not part of Player Stats acceptance. Reconsider only when U.GG exposes a stable
anonymous JSON contract covering the complete `PlayerStatsProvider` surface. See
`docs/ugg-chrome-api-investigation.md`.
