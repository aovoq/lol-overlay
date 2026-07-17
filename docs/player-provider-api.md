# Player provider API map

Recorded 2026-07-13; refreshed 2026-07-17. Player providers are selected explicitly and never
fall back to another site.
CAPTCHA, authentication, and bot mitigations are not bypassed.

## DeepLoL

- Host: `https://b2c-api-cdn.deeplol.gg`; anonymous JSON `GET` reads.
- Identity: `/summoner/summoner` with `platform_id`, `riot_id_name`, and
  `riot_id_tag_line`; returns PUUID/basic profile. `KR` is the only unnumbered platform.
- Current rank: `/summoner/summoner-realtime` requires both `summoner_id` and `puu_id`. An empty
  resolver `summoner_id` remains a valid query value when the PUUID is present, so it is sent
  verbatim instead of suppressing the rank request.
- Match IDs: `/match/matches` with PUUID/platform, `only_list=1`, offset, and filters. The observed
  unit is 20; the next offset uses the actual returned count.
- Details: `/match/match-cached`, one request per match ID, hydrated with concurrency 5 and partial
  failure reporting.
- Champion stats: `/summoner/champion-stat`, narrowed to normalized rows and cached separately.
- Tier chart: `/summoner/tier-chart`; `last_match_id` is required despite older documentation.
- Freshness: `/summoner/updated-time` supplies the last update and any server delay. The adapter
  also derives the official 45-second cooldown from `updated_timestamp` so an app restart cannot
  bypass it.
- Site refresh: `POST renew.deeplol.gg/match/check-refresh` must return `available` before the
  adapter calls `refresh_tier`, `refresh-matches`, and `refresh-champion-stat`. Concurrent and
  repeated refreshes are blocked before mutation; success is followed by cache invalidation and
  fresh reads.
- Expected errors: JSON 404 player missing, 422 invalid/missing input, 429 with optional
  `Retry-After`; request timeout is typed separately.

## OP.GG

- Profile/champion host: `https://mcp-api.op.gg/mcp`; anonymous JSON-RPC 2.0 `POST` with JSON
  content type. The official summoner profile response is compact structured JSON constructors,
  not rendered HTML.
- Identity: Riot ID plus region; PUUID from the profile result is used for participant selection.
- Matches: anonymous `POST` to the public profile route with `Accept: text/x-component`,
  `Content-Type: text/plain;charset=UTF-8`, and the current `Next-Action` identifier for
  `getGames`. The JSON argument contains locale, lowercase region, PUUID, game type, `endedAt`, and
  nullable champion. No cookie or login header is required.
- Action discovery: fetch the public profile HTML, follow its first-party `c-lol-web.op.gg`
  JavaScript bundle references, locate the `getGames`, `renewalStatus`, and `renewal` server
  references, and cache their deployment identifiers. This avoids hard-coded build hashes.
- Pagination: each action result contains up to 20 structured games and
  `meta.last_game_created_at`. That timestamp becomes the next `endedAt` cursor; live acceptance
  returned 20 + 20 distinct chronological records.
- Refresh: the official MCP exposes no mutation, but the first-party profile app exposes structured
  `renewalStatus` and `renewal` server actions. The adapter checks `renewableAt` before mutation,
  sends one anonymous renewal request, polls only at the returned delay until
  `RENEWAL_FINISH`, then invalidates its caches. Renewal calls are serialized and protected by the
  server cooldown plus a 60-second local minimum; `TOO_MANY_RENEWALS` maps to typed 429 behavior.
- Rate-limit hypothesis: HTTP 429 and `Retry-After` are honored; no stable published quota was
  found.

## U.GG player pages

- Candidate direct endpoint: `POST https://u.gg/api`, GraphQL operation
  `FetchMatchSummaries`, using page-number pagination. The public page also renders profile, rank,
  and champion statistics from GraphQL-shaped data.
- Required browser headers/cookies and a stable profile query could not be established: direct
  requests from the acceptance environment receive Cloudflare `403` with
  `cf-mitigated: challenge`/Turnstile before GraphQL executes.
- The older `stats2.u.gg` versioned JSON endpoints are champion-wide build statistics and do not
  provide a Riot-ID player resolver.
- No U.GG player adapter is registered until anonymous direct JSON can be fixture- and live-tested.
  HTML/DOM parsing and challenge bypass are prohibited by the execution contract.
- Re-evaluate when U.GG publishes an anonymous player JSON contract, or the challenge is removed
  for normal direct clients. Record query variables, IDs, pagination, and 429 behavior before
  enabling it.
