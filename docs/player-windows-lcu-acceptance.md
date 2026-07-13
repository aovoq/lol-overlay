# Player Stats Windows / LCU manual acceptance

Status: **prepared, not executed**. This gate requires Windows and a running League Client. It was
authored and reviewed on macOS on 2026-07-14 JST; no Windows, LCU, or Tauri UI result is claimed.

## Purpose and pass condition

This is the independent target-platform gate for Player Stats. It verifies that LCU supplies only
the logged-in identity, that the selected external provider performs the Summoners search, and that
the existing Home `summoner` and `match-history` event behavior has not regressed. It also exercises
the real Tauri command/UI path that browser-only E2E cannot cover.

Pass only when every checkbox in the execution record is completed on the same Windows run. U.GG
must remain available for Build data but absent from every Player Stats selector and result.

## Prerequisites

- Windows 10 or 11 on the target architecture.
- League of Legends and Riot Client installed. Log into League, remain on the Home/lobby screen,
  and use an account whose Riot ID has a non-empty game name and tag.
- Git checkout containing the recorded Player Stats completion commits, with no unrelated local
  changes overwritten.
- Bun 1.3.13-compatible runtime, Rust stable toolchain with the Windows target, and WebView2.
- Anonymous outbound HTTPS access to DeepLoL and OP.GG. Do not add cookies, solve/bypass a
  challenge, or proxy U.GG Player traffic.

Record Windows version, commit, League patch, platform ID, and the test Riot ID. A public test
account is preferable; redact the Riot ID from shared logs if required.

## Preparation commands

Run from the repository root in PowerShell:

```powershell
git status --short --branch
git rev-parse --short HEAD
bun install
bun run check
bun run test:e2e
cargo test -p overlay-provider-deeplol --lib
cargo test -p overlay-provider-opgg --lib
cargo test -p overlay-provider-ugg --lib
cargo test -p overlay-lcu --lib rest::tests::windows_lcu_player_identity_manual_harness -- --ignored --nocapture
```

The ignored harness is read-only. It reads current summoner, platform, and up to five recent LCU
matches and must print `WINDOWS LCU IDENTITY OK`. It does not call external Player providers and
does not mutate runes, spells, the client, or site data.

Then start the desktop shell and keep the terminal visible:

```powershell
bun run tauri dev
```

## Tauri UI steps and expected results

1. Open **HOME** immediately after startup.
   - The logged-in summoner identity, rank when available, and recent non-remake matches appear.
   - Enable Developer Mode in Settings and open the debug event log. It contains non-null
     `summoner` and an array-valued `match-history` event. These are the pre-existing Home events;
     Player Stats must not rename, suppress, or replace them.
2. Open **SUMMONERS** without typing a Riot ID.
   - The page auto-searches exactly the LCU `platform + gameName#tagLine` reported by the harness.
   - The displayed Profile/Rank/Matches/Champion Stats source is the selected external provider,
     not an LCU fallback. An external error is shown as an error; Home data is not substituted.
3. Inspect the Player provider selector and Settings → **PLAYER STATS**.
   - Exactly DeepLoL and OP.GG are selectable. U.GG and LoLalytics are absent.
   - Settings → **BUILD DATA** still includes U.GG.
4. With DeepLoL selected, search a known Riot ID and verify Profile, current rank when the upstream
   resolver supplies it, previous tier, 20 matches, **load more** for the next page, Champion Stats,
   queue filters, participant details, and **再読み込み**.
   - Refresh is cache invalidation plus read refetch; no site mutation control is shown.
   - A partial page keeps successful matches visible and exposes retry for failed match IDs.
5. Switch Player Stats to OP.GG.
   - The same Riot ID reloads from OP.GG. No DeepLoL rows remain mixed into the view.
   - Load more returns the next chronological page, not a duplicate first page.
6. Navigate away and back, then quit and relaunch `bun run tauri dev`.
   - OP.GG remains selected for Player Stats while the Build source remains independently selected.
   - With LCU connected, the current identity auto-searches again. With League closed, the last
     search is restored and the form remains usable.
7. Return to **HOME** after both provider switches and a Player refresh.
   - Summoner/rank and recent match history still update.
   - The debug log still records `summoner` and `match-history`; `player-stats-source` appears only
     for Player selector changes and does not replace either Home event.

## Failure probes

- Search a syntactically valid, nonexistent Riot ID: a not-found state appears and prior profile
  data is cleared.
- Enter an invalid Riot ID without `#tag`: validation prevents a request.
- If an upstream naturally returns 429, record the displayed retry delay. Do not intentionally
  flood any provider to manufacture a rate limit.
- Close League while the app is running: Home may report LCU unavailable, but Summoners remains an
  external-provider feature and must not silently mix stale LCU data.

## Evidence and failure logs

Capture the following without secrets or LCU authorization material:

- PowerShell output for the harness and `bun run tauri dev`.
- Screenshots of Home, DeepLoL ready state, OP.GG ready state, Settings selectors, and one expected
  error state.
- Developer debug event entries for `summoner`, `match-history`, `player-stats-source`, and any
  `log` warning. The log is capped, so capture it promptly.
- Browser/WebView developer-console command errors if a Tauri invoke fails.
- The exact step, selected provider, platform, timestamp, and visible typed error. Never include the
  League lockfile password, cookies, tokens, or full private account history.

If identity auto-search fails, first compare the harness `platform` and Riot ID with the form. If
Home events fail, capture both event names and the terminal output. If only one external provider
fails, rerun that provider's ignored live acceptance once and record the upstream status/content
type; do not enable fallback or U.GG Player support.

## Execution record

- [ ] Windows/League prerequisites recorded
- [ ] Offline commands pass
- [ ] Read-only ignored LCU harness prints `WINDOWS LCU IDENTITY OK`
- [ ] LCU identity auto-search passes
- [ ] Existing Home `summoner` and `match-history` events pass before and after Player operations
- [ ] DeepLoL Tauri UI flow passes
- [ ] OP.GG Tauri UI flow passes
- [ ] Player setting persists independently across restart
- [ ] U.GG remains Build-only and absent from Player UI
- [ ] Evidence paths and any failures recorded

Execution date: **not executed**  
Tester / machine: **not executed**  
Result: **pending Windows/LCU manual execution**
