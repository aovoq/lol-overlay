# Player Stats independent review

Review date: 2026-07-14 JST. Authoritative contract: `docs/provider-status.html` (335 lines).
Scope reviewed: all 70 local commits, the 20 commits ahead of `origin/main`, the current worktree,
Player DTO/router/proxies, DeepLoL/OP.GG/U.GG adapters, Tauri commands, frontend state/UI, fixtures,
documentation, offline tests, Playwright E2E, and safe anonymous live paths.

## Verdict

**Blocked.** DeepLoL and OP.GG pass fresh anonymous live acceptance for profile/rank, 20 + 20
matches, champion stats, and read refresh. U.GG still has no compliant executable anonymous direct
player JSON path from this environment. Both `GET` and GraphQL `POST` to `https://u.gg/api` are
stopped before application execution by Cloudflare (`403`, HTML, `cf-mitigated: challenge`). No
Turnstile automation, authenticated cookie reuse, rendered-HTML data extraction, or challenge
bypass was attempted.

Exact unblock condition: U.GG must expose a stable anonymous player JSON contract reachable by a
normal direct client without Cloudflare challenge/authentication, or remove the challenge from the
existing GraphQL player operations. The contract must cover Riot-ID resolution, profile/ranks,
20-item match pagination, champion stats, errors/rate limits, and read refresh so fixtures and a
live PlayerStatsProvider acceptance test can pass.

## Confirmed issues remediated

| Severity | Before | After |
| --- | --- | --- |
| High | DeepLoL substituted the first participant when the searched PUUID was absent. | The match becomes a typed partial failure; another player's stats can never be presented as the target. |
| High | OP.GG normalized regions by stripping a trailing `1`, producing invalid `OC`, `LA`, and `LA2`. | Explicit Riot-to-OP.GG mapping covers OCE/LAN/LAS/EUNE and all advertised regions; unsupported platforms fail validation. |
| High | One OP.GG page load could race three identical profile MCP requests. | Five-minute provider cache plus single-flight coalesces profile/match/champion reads, including concurrent force refreshes. |
| High | Player outputs had no provider-neutral validation. | Proxy rejects wrong-source payloads, invalid/duplicate match IDs, invalid rates/counts, and malformed refresh results. |
| Medium | Tauri player failures crossed as untyped strings. | Player commands return camelCase `{ kind, message, retryAfter }` errors; frontend handles typed and legacy errors. |
| Medium | A failed new search retained the previous player's profile; load-more/refresh failures were silent. | Search clears stale data and operation failures enter a visible error state with regression tests. |
| Medium | Transient timeouts/connect/5xx failures had no bounded retry. | DeepLoL and OP.GG player reads retry once after 250 ms; 429 remains immediate and preserves Retry-After. |
| Medium | OP.GG ladder fraction was displayed as a percentage without conversion. | Adapter converts rank/total to percentage units and validates `0..=100`. |
| Low | UI omitted seven advertised regions and partial-retry E2E never retried. | All advertised regions are selectable and E2E proves partial recovery and full-history deletion. |
| Low | An inert site-mutation button existed with no safe command, and shimmer ignored reduced motion. | Mutation affordance is hidden until a safe provider contract exists; reduced-motion disables shimmer. |

Focused remediation commits: `3df1df9`, `cba8a00`, `91f0ad8`, `6f37cd7`.

## 32-task audit

| # | Task | Status | Independent evidence / residual gap |
| ---: | --- | --- | --- |
| 1 | Generic ProviderRouter | Pass | Router registration/active/unregistered tests pass. |
| 2 | BuildProviderProxy rename | Pass | All build endpoints have forwarding coverage. |
| 3 | Build/player settings split | Pass | Independent fields, legacy migration, persistence path verified. |
| 4 | Provider capabilities | Pass | Player list excludes unsupported providers; OP.GG `directApi` corrected to false. |
| 5 | PlayerStatsProxy | Pass | Independent routing, no fallback, single-flight, cache separation, and validation pass. |
| 6 | U.GG/OP.GG network map | Blocked | OP.GG documented and live; U.GG cannot pass Cloudflare to establish a full request contract. |
| 7 | Player trait/DTO | Pass | Trait, camelCase serde DTOs, extras, cursors, and refresh result are connected. |
| 8 | LCU initial identity | Partial | Code composes current summoner + platform and UI auto-searches; real Windows LCU acceptance was unavailable on this macOS host. |
| 9 | U.GG PlayerStatsProvider | Blocked | Not registered; only champion-build JSON tests exist. |
| 10 | DeepLoL resolver/profile | Partial | Fresh KR live pass and empty-ID fixture; explicit offline resolver URL-encoding/404 request fixtures are absent. |
| 11 | DeepLoL matches/hydration | Partial | 20 + 20 live, concurrency, cache, target-PUUID and partial parsing pass; no mock-HTTP timeout/partial page suite. |
| 12 | DeepLoL champion stats | Partial | Reduced inline live-schema fixtures and separate cache exist; no checked-in raw-schema sample fixture. |
| 13 | DeepLoL tier chart | Pass | Missing-ID 422 and supplied-ID JSON fixtures; live tier chart derived from latest match. |
| 14 | DeepLoL realtime rank | Pass with constraint | Non-empty `summoner_id` works live; empty-ID behavior and alternative display are recorded in ADR. |
| 15 | DeepLoL refresh boundary | Pass | Read client never calls authenticated renew host; no site mutation is advertised. |
| 16 | DeepLoL contract fixtures | Partial | Malformed/content-type/422/429/empty/special queue/live schema covered; the complete fixture matrix is absent. |
| 17 | DeepLoL platform map | Partial | Normalization unit coverage and KR live pass; advertised non-KR regions lack live fixtures. |
| 18 | Tauri player commands | Partial | Commands registered, camelCase DTO and typed-error snapshots pass; direct invocation tests are absent. |
| 19 | Frontend player state | Partial | Core races/pagination/errors tested; not every transition has an isolated unit test. |
| 20 | Summoners page | Partial | Main states/responsive keyboard-native controls pass E2E; Windows Tauri/LCU UI acceptance unavailable. |
| 21 | Search validation/history | Pass | Parser, preservation, dedupe, cap, corrupt storage, deletion, and restore covered. |
| 22 | Settings UI split | Partial | Independent selectors and backend migration work; dedicated frontend persistence/event coverage is absent. |
| 23 | Player E2E | Pass | Four headless scenarios cover required flows including 404/422/429 and real partial retry; no fixed waits. |
| 24 | Three-provider final live | Blocked | DeepLoL and OP.GG pass; U.GG player adapter cannot execute. |
| 25 | OP.GG API gate | Pass with risk | MCP JSON-RPC plus structured first-party Flight action is documented/live; action discovery remains upstream-sensitive. |
| 26 | Cache/pagination/load control | Pass | TTL, single-flight, separation, force, 429, retry, and both cursor strategies verified. |
| 27 | Common field semantics | Pass | Four-provider difference table, unknown/null and fallback policy documented. |
| 28 | DeepLoL failure diagnostics | Pass | Status/content type/safe bounded body and counter/rune causes covered. |
| 29 | Zero vs unknown | Pass | Build DTO uses Option and UI renders unknown distinctly. |
| 30 | Provenance metadata | Pass | Required metadata and UI tooltips verified. |
| 31 | Shared normalizer | Pass | Build suite plus player-output validator reject malformed values. |
| 32 | Cross-provider contract tests | Partial | One-line build suite covers four providers; no complete PlayerStats suite exists and U.GG player is absent. |

The previous `30 / 32` claim is not accepted. Two tasks are externally blocked and ten additional
tasks are only partially evidenced against their exact Done When text. These partial rows are
residual test/acceptance gaps, not permission to bypass U.GG protections.

## Verification evidence

Passed after remediation:

```text
bun run check
# format, Biome, Clippy -D warnings, TypeScript, 49 Vitest tests, workspace Rust tests

bun run test:e2e
# 4 passed, Chromium, no fixed waits

cargo test -p overlay-provider-deeplol --lib
# 23 passed, 10 ignored

cargo test -p overlay-provider-opgg --lib
# 22 passed, 5 ignored

cargo test -p overlay-provider-ugg --lib
# 16 passed, 2 ignored

cargo test -p overlay-provider-deeplol --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
# pass: profile, tier chart, 20 + 20 matches, 76 champion rows

cargo test -p overlay-provider-opgg --lib player::tests::live_player_stats_acceptance -- --ignored --nocapture
# pass: profile, 3 rank rows, 20 + 20 matches, 10 champion rows

cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
# 2 build-statistics tests pass; no player test exists
```

Residual risks, highest first:

1. U.GG Player Stats and three-provider final acceptance remain impossible under the current challenge.
2. Several exact offline fixture/command/state requirements remain partial as listed above.
3. OP.GG match pagination depends on an undocumented first-party server action and bundle discovery.
4. DeepLoL current rank is unavailable for profiles whose resolver omits `summoner_id`.
5. No provider exposes a compliant anonymous site mutation; only app/read refresh is available.
6. Real LCU and target-platform acceptance require a running League client on Windows.

Pre-existing untracked files `docs/provider-status.html` and `apps/mobile/docs/` were preserved and
not added to remediation commits. No GitHub, push, PR, release, paid service, authenticated cookie,
or system-wide dependency operation was used.
