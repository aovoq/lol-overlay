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
The adapter treats a cursor as unsupported. Returning the same matches with a synthetic cursor
would corrupt pagination semantics, so load-more remains unavailable for OP.GG until the official
surface adds continuation support.

## U.GG player provider

Implementation is gated on direct JSON. The currently observable GraphQL endpoint is intercepted by
a managed Cloudflare challenge in the live environment, and stable anonymous profile operations are
not documented. The project will not parse rendered HTML, automate Turnstile, borrow user cookies,
or evade bot protection. Consequently U.GG player support remains unregistered and the overall
three-provider acceptance goal remains open.

