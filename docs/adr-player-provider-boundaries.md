# ADR: player-provider upstream boundaries

Status: accepted, 2026-07-13.

## DeepLoL realtime rank

`summoner-realtime` requires a `summoner_id` omitted by the older explorer contract. Some valid
basic profiles return it as an empty string. The adapter therefore calls realtime only when the
resolver returns a non-empty ID. Otherwise current Solo/Flex rank remains unavailable while basic
profile and previous tiers are shown. It does not infer the ID or mix LCU/another provider's rank.

## DeepLoL refresh

Read freshness and site mutation are separate capabilities. The Refresh button invalidates app
caches and forces anonymous reads. It never posts to `renew.deeplol.gg`; `siteRefresh=false` until
an authenticated, cooldown-aware contract is deliberately implemented.

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
