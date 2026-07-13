# Player provider API map

Recorded 2026-07-13. Player providers are selected explicitly and never fall back to another site.
CAPTCHA, authentication, and bot mitigations are not bypassed.

## DeepLoL

- Host: `https://b2c-api-cdn.deeplol.gg`; anonymous JSON `GET` reads.
- Identity: `/summoner/summoner` with `platform_id`, `riot_id_name`, and
  `riot_id_tag_line`; returns PUUID/basic profile. `KR` is the only unnumbered platform.
- Current rank: `/summoner/summoner-realtime` also requires `summoner_id`. It is called only when
  the resolver returns a non-empty value.
- Match IDs: `/match/matches` with PUUID/platform, `only_list=1`, offset, and filters. The observed
  unit is 20; the next offset uses the actual returned count.
- Details: `/match/match-cached`, one request per match ID, hydrated with concurrency 5 and partial
  failure reporting.
- Champion stats: `/summoner/champion-stat`, narrowed to normalized rows and cached separately.
- Tier chart: `/summoner/tier-chart`; `last_match_id` is required despite older documentation.
- Freshness: `/summoner/updated-time` is a read. Mutation endpoints are on
  `renew.deeplol.gg`, require authentication, and are not called by this provider.
- Expected errors: JSON 404 player missing, 422 invalid/missing input, 429 with optional
  `Retry-After`; request timeout is typed separately.

## OP.GG

- Host: `https://mcp-api.op.gg/mcp`; anonymous JSON-RPC 2.0 `POST` with JSON content type.
- Methods used: official OP.GG MCP summoner profile, match-history, match-detail, and champion
  analysis tools. Responses are compact structured JSON constructors, not rendered HTML.
- Identity: Riot ID plus region; PUUID from the profile result is used for participant selection.
- Pagination: the official tool currently exposes a maximum of 20 matches and no cursor. A cursor
  request returns an explicit unsupported error; the adapter does not replay page one or invent an
  offset.
- Refresh: application cache invalidation/read only. The official MCP surface exposes no safe site
  mutation.
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

