# Player Stats completion review

Review date: 2026-07-14 JST. Authoritative product contract: commit `b1f2b67` plus
`docs/provider-status.html`. Player Stats has exactly two providers: DeepLoL and OP.GG. U.GG is a
Build Provider only and is intentionally absent from Player registration, settings, and Player live
acceptance.

## Verdict

**All locally actionable Player Stats requirements pass.** The original independent review's
fixture, command, state, settings, and cross-provider evidence gaps were independently reproduced
and closed in `c1c530e`, `faf8d02`, `919da32`, and `f6ff92d`. Fresh offline, E2E, and anonymous live
acceptance passed on 2026-07-14.

The Windows/LCU target-platform gate is **prepared but not executed**. This work ran on macOS and
does not claim a Windows, League Client, or real Tauri UI result. The read-only ignored harness,
prerequisites, exact commands, UI steps, expected events/results, and evidence procedure are in
`docs/player-windows-lcu-acceptance.md`.

## Closed independent findings

| Finding | Evidence now present |
| --- | --- |
| DeepLoL resolver encoding and 404 | A loopback mock HTTP test asserts encoded Unicode, slash, spaces, `#`, JP→JP1, and typed JSON 404. |
| Timeout, partial page, pagination | Real `reqwest` loopback fixtures prove bounded retry→typed timeout, 19 successes + 1 hydration failure, special queue propagation, offset 20, and a short final page. |
| Raw champion schema | `crates/provider-deeplol/fixtures/player_champion_stats_raw_sample.json` is checked in and consumed by parser and shared-contract tests. |
| Failure/queue/content type matrix | Offline matrix covers JSON/vendor JSON/missing type, HTML, empty/malformed bodies, 404/422/429/500/502, Retry-After, and standard/special queue IDs. |
| Platform mapping | All advertised mappings are fixture-tested without broad live traffic; KR remains covered by the representative live acceptance. |
| Tauri commands | Direct managed-state tests call source/list/profile/matches/champion/refresh commands, verify camelCase payloads, all five typed errors, independent persistence, and U.GG rejection. |
| Frontend transitions/races | Isolated tests cover idle/loading/ready/empty/partial/error, all error kinds, filters, provider switch, stale search/load-more/refresh, and duplicate pagination. |
| Player settings | A testable controller proves allowed choices, persisted selection, event updates, rollback on persistence failure, and U.GG filtering. |
| Player contract suite | One macro invocation per adapter validates parser-produced DeepLoL and OP.GG profile/pages/champions/refresh/capabilities, units, pagination, ordering, missing values, and provenance. |
| U.GG boundary | Production registration, direct commands, frontend state/settings, and Playwright all assert that U.GG is absent from Player Stats; U.GG offline and ignored Build tests pass. |

## Reconciled 32-task audit

| # | Task | Status | Completion evidence |
| ---: | --- | --- | --- |
| 1 | Generic ProviderRouter | Pass | Registration, active routing, ordering, and unregistered rejection tests. |
| 2 | BuildProviderProxy rename | Pass | Every Build endpoint forwards through the active provider without fallback. |
| 3 | Build/player settings split | Pass | Independent fields, legacy `dataSource` migration, persisted Player selection. |
| 4 | Provider capabilities | Pass | Capability-filtered descriptors; OP.GG `directApi=false`; U.GG absent. |
| 5 | PlayerStatsProxy | Pass | Independent routing/cache/single-flight/validation; no cross-provider fallback. |
| 6 | U.GG/OP.GG network map | Pass | OP.GG executable contract and U.GG Build-only Cloudflare boundary/re-evaluation condition documented. |
| 7 | Player trait/DTO | Pass | camelCase DTOs, nullable fields, tagged extras, cursors, partial failures, refresh. |
| 8 | LCU initial identity | Local Pass; Windows gate not executed | Code and read-only harness cover summoner+platform composition; reproducible real-client gate prepared. |
| 9 | U.GG Player boundary | Pass | Deliberately no Player trait implementation/registration/choice/live test; Build tests pass. |
| 10 | DeepLoL resolver/profile | Pass | 200 schema, empty ID/name behavior, loopback URL encoding/404, and live profile. |
| 11 | DeepLoL matches/hydration | Pass | 20+20 live; mock timeout, partial page, cursor, cache, concurrency, target PUUID. |
| 12 | DeepLoL champion stats | Pass | Checked-in representative raw schema, reduced parser fixtures, separate cache, 76-row live result. |
| 13 | DeepLoL tier chart | Pass | Missing-ID 422, supplied-ID JSON fixture, latest-match derivation, live enrichment. |
| 14 | DeepLoL realtime rank | Pass with documented constraint | Non-empty `summoner_id` path; empty-ID unknown state and no inferred/mixed rank. |
| 15 | DeepLoL refresh boundary | Pass | App cache/read refresh only; no authenticated renew-host mutation. |
| 16 | DeepLoL contract fixtures | Pass | Complete status/body/content-type/queue matrix plus top-level and live schema checks. |
| 17 | DeepLoL platform map | Pass | All advertised mapping fixtures plus KR representative live; no unsafe broad regional sweep. |
| 18 | Tauri player commands | Pass | Direct command invocation, registration, DTO/error serde, persistence, U.GG rejection. |
| 19 | Frontend player state | Pass | Every view state and all request-generation races isolated in Vitest. |
| 20 | Summoners page | Local Pass; Windows gate not executed | Responsive/keyboard flows pass E2E; real Tauri UI checklist fully prepared. |
| 21 | Search validation/history | Pass | Parser, case/space preservation, dedupe, cap, corrupt storage, restore/delete. |
| 22 | Settings UI split | Pass | Independent choices, persistence, migration, event update/rollback, U.GG exclusion. |
| 23 | Player E2E | Pass | Four Chromium scenarios: auto/arbitrary/restored search, provider switch, pages, partial retry, refresh, typed failures, responsive UI. |
| 24 | Two-provider final live | Pass | Fresh DeepLoL and OP.GG Player live tests plus production U.GG-unregistered assertion. |
| 25 | OP.GG API gate | Pass with upstream risk | Official MCP plus structured anonymous first-party Flight action and dynamic action discovery. |
| 26 | Cache/pagination/load control | Pass | TTL, single-flight, force, retry, 429, duplicate suppression, offset/timestamp cursors. |
| 27 | Common field semantics | Pass | Units, unknown/null, provider separation, fallback/provenance policy documented. |
| 28 | DeepLoL failure diagnostics | Pass | Status/content type/bounded safe body and parser causes covered. |
| 29 | Zero vs unknown | Pass | Optional Build values and Player nullable values remain distinct through UI. |
| 30 | Provenance metadata | Pass | Source/freshness and provider-specific extras preserved and validated. |
| 31 | Shared normalizer | Pass | Build normalization plus Player output/contract validators reject malformed values. |
| 32 | Cross-provider contract tests | Pass | Reusable Player macro runs for DeepLoL and OP.GG; production registry asserts U.GG absent. |

### Count reconciliation

The previous `30 / 32` statement and the later `2 blocked + 10 partial` audit used the obsolete
three-Player-provider assumption and mixed implementation evidence with target-platform execution.
Under the authoritative `b1f2b67` two-provider contract:

- 30 tasks are unconditional Pass.
- Tasks 8 and 20 are locally Pass and share one external Windows/LCU manual gate.
- 0 tasks retain an implementation Partial.
- 0 tasks are blocked by U.GG; Build-only exclusion is the required completed state.
- Therefore all 32/32 locally actionable tasks are complete, while one Windows/LCU manual gate is
  explicitly pending execution.

## Fresh verification evidence

Execution window: 2026-07-14 01:56–02:00 JST on macOS.

```text
bun run format
# pass: Biome, rustfmt, Taplo

bun run check
# pass: format check, Biome, Clippy -D warnings, TypeScript/workspaces,
# 56 Vitest tests, all workspace Rust unit tests

bun run test:e2e
# 4 passed, Chromium, 2.6s; provider switch is OP.GG and U.GG option absence is asserted

cargo test -p overlay-provider-deeplol --lib
# 28 passed, 10 ignored

cargo test -p overlay-provider-opgg --lib
# 23 passed, 5 ignored

cargo test -p overlay-provider-ugg --lib
# 16 passed, 2 ignored; Build-only crate

cargo test -p overlay-provider-deeplol --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
# DEEPLOL PLAYER LIVE OK: profile=Hide on bush first=20 second=20 champions=76

cargo test -p overlay-provider-opgg --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
# OPGG PLAYER LIVE OK: profile=Hide on bush ranks=3 first=20 second=20 champions=10

cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
# 2 passed; U.GG Build statistics only, no Player adapter/test
```

## Remaining external risks

1. The Windows/LCU manual gate has not been executed.
2. OP.GG match pagination depends on an undocumented first-party server action and bundle discovery.
3. DeepLoL current rank is unknown when its resolver omits `summoner_id`.
4. Neither Player provider exposes a compliant anonymous site mutation; refresh is app cache/read only.
5. External provider contracts can change after this dated live evidence.

Pre-existing untracked `apps/mobile/docs/` was preserved. No Mobile file, GitHub operation, push,
PR, release, challenge bypass, authenticated cookie, or paid service was used.
