# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A lightweight League of Legends overlay built with Tauri (Rust backend + SolidJS frontend). It never injects into or reads the game's memory — it only reads Riot's official local APIs and draws a transparent, always-on-top, click-through window over the (borderless) game. Two features: in-game item recommendations and automatic rune-page import during champ select.

## Commands

```bash
pnpm install
pnpm tauri dev      # run the app (Vite dev server + Tauri)
pnpm tauri build    # distributable build (run on Windows)

pnpm dev            # frontend only (Vite, no Tauri shell)
pnpm build          # tsc typecheck + vite build
pnpm format         # Biome + rustfmt + Taplo write fixes
pnpm format:check   # check TS/CSS/JSON, Rust, and TOML formatting
pnpm lint           # Biome lint + strict Clippy
pnpm check          # format check + lint + typecheck + Rust unit tests

# Rust tests (from repo root — Cargo workspace)
cargo test --workspace --lib
# Network-dependent end-to-end checks:
cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture
cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
cargo test -p overlay-provider-lolalytics --lib -- --ignored --nocapture
cargo test -p overlay-provider-opgg --lib -- --ignored --nocapture
```

**Target platform is Windows.** UI/frontend work runs fine on Mac, but anything touching the LCU or Live Client Data API requires a running LoL client (Windows).

## Releasing

Releases are built by GitHub Actions (`.github/workflows/release.yml`) for macOS (Apple Silicon) and Windows, published to GitHub Releases, and picked up by the in-app auto-updater (`tauri-plugin-updater`, checked from the control window at startup).

```bash
git tag v0.2.0 && git push --tags   # that's the whole release
```

- **The git tag is the single source of truth for the version.** The workflow strips the `v` and injects it via `tauri build --config`; do NOT bump `version` in `tauri.conf.json` / `package.json` / `Cargo.toml` (those stay as dev placeholders).
- The workflow signs updater artifacts with the `TAURI_SIGNING_PRIVATE_KEY` repo secret (key has no password; the matching pubkey lives in `tauri.conf.json` under `plugins.updater`). If that key is ever lost, existing installs can no longer update.
- `latest.json` on the latest release is the updater manifest; clients poll `releases/latest/download/latest.json`.
- Auto-update can only be verified end-to-end across two releases: install version N, then tag N+1 and confirm the update dialog appears.

## Formatting and linting

- Frontend formatting/linting uses **Biome** (`biome.json`) for TS/TSX/CSS/JSON/HTML.
- Type checking is still **TypeScript** (`pnpm typecheck`); Biome does not replace `tsc`.
- Rust formatting uses **rustfmt** with LF newlines (`rustfmt.toml`).
- Rust linting uses **Clippy** via `cargo clippy --workspace --all-targets -- -D warnings`.
- TOML formatting uses **Taplo** (`.taplo.toml`) for the workspace `Cargo.toml` files.
- Run `pnpm check` before handing off a non-trivial change.
- Use `pnpm format` to apply formatter changes instead of hand-formatting large diffs.
- `.gitattributes` pins text files to LF. Do not introduce CRLF-only churn.

## Architecture

The repo is a **Cargo workspace**. The Tauri app (`src-tauri/`) orchestrates background tasks and emits events; shared logic lives in `crates/`.

```
lol-overlay (src-tauri)     engine, commands, events, hittest, hotkeys, mock
 ├── overlay-types          shared serde types (events, provider payloads, LCU)
 ├── overlay-lcu            irelia wrapper (irelia dependency stops here)
 ├── overlay-live-client    Live Client Data API client
 ├── overlay-ddragon        Data Dragon client + champion/item maps (shared)
 ├── overlay-provider       BuildProvider trait, ProviderProxy, hardcoded fallback
 ├── overlay-provider-deeplol
 ├── overlay-provider-ugg
 ├── overlay-provider-lolalytics
 └── overlay-provider-opgg
```

The frontend (`src/`, SolidJS) listens for Tauri events and renders panels into `index.html`.

### Two Riot data sources

- **LCU API** (`overlay-lcu`) — the League *client* (lobby, champ select, runes). Accessed via [`irelia`](https://github.com/AlsoSylv/Irelia) inside `overlay-lcu` only. Used for gameflow phase, champ-select session, and writing rune pages (`/lol-perks/*`).
- **Live Client Data API** (`overlay-live-client`) — read-only REST on `https://127.0.0.1:2999` while a *match* is running. No auth, self-signed cert. Has **no WebSocket**, so in-game state is polled.

### Engine orchestration (`engine.rs`, wired up in `lib.rs::run`)

Three concurrent pieces share the channel `tokio::sync::mpsc`:

1. **WebSocket subscriber** (`overlay_lcu::subscribe_champ_select`) pushes champ-select session updates onto the channel as they happen. The socket handle is intentionally leaked inside `overlay-lcu` so the subscription lives for the process lifetime.
2. **`rune_processor`** drains the channel and, on a *pick change* (deduped by champion id), looks up runes via the provider and writes them through the LCU. The same dedup makes duplicate WS + REST sessions harmless.
3. **`poller`** (every 2s) tracks gameflow phase + in-game state for the UI, AND re-pushes the champ-select session onto the channel as a REST fallback in case the WebSocket missed an update. In-game item recommendations are produced here (Live Client API has no WS).

### Backend → frontend events (Tauri `emit` / `listen`)

`phase`, `champ-select`, `recommendations`, `summoner`, `match-history`, `lp-change`, `rune-imported`, `window-mode`, `interactive`, `log`, `data-source`, `mock-stage`. Payload structs live in `events.rs` / `overlay-types` and are mirrored as TypeScript interfaces in `src/types.ts`. **All payloads use `#[serde(rename_all = "camelCase")]`** — keep both sides in sync when changing a payload.

Frontend → backend commands (`commands.rs`): settings/layout setters, data-source switching (`get_data_source`, `list_data_sources`, `set_data_source`), OPENLOL data lookups (`get_tier_list`, `get_rune_build`, `get_counters`, `import_build`), click-through plumbing (`set_hit_regions`, `set_drag_active`, `set_interactive`), and developer mode (`set_developer_mode`, `get_mock_stage`, `set_mock_stage` — the settings-panel toggle that shows the debug panel and unlocks mock scenarios).

### Data provider abstraction (`overlay-provider`)

Everything the overlay needs "from the internet" flows through the `BuildProvider` trait. Runtime routing goes through **`ProviderProxy`**, which forwards to the active backend (`ProviderKind`: `deeplol` | `ugg` | `lolalytics` | `opgg`). Every backend shares one `Arc<DdragonClient>` for static maps.

- **`overlay-provider-deeplol`** (`DeepLolProvider`) — DeepLoL CDN + Data Dragon. Full OPENLOL support (tier list, counters, matchup runes).
- **`overlay-provider-ugg`** (`UggProvider`) — u.gg stats2 API. In-game items/skills/runes + counters; tier list returns `NotEnoughData` (no site-wide tier JSON on stats2).
- **`overlay-provider-lolalytics`** (`LolalyticsProvider`) — LoLalytics' internal `mega` JSON API (`a1.lolalytics.com/mega/?ep=…`; needs a `Referer: https://lolalytics.com/` header). Supports items (`ep=build-itemset`/`build-earlyset`), counters (`ep=counter`), and tier list (`ep=tier`). **Runes/skills/spells return `NotEnoughData`** — LoLalytics serves the primary build object only inside server-rendered HTML, with no clean JSON endpoint. Uses `patch=30` (last-30-days aggregate, no version lookup), `tier=platinum_plus`, `region=all`. Champion slug = the Data Dragon alias lowercased (`MonkeyKing` → `monkeyking`).
- **`overlay-provider-opgg`** (`OpggProvider`) — op.gg has no public JSON API at all; every value is recovered from the Next.js "flight" (React Server Component) payload embedded in the server-rendered HTML of `op.gg/lol/champions/{slug}/build[/lane]`, `.../counters[/lane]`, and `op.gg/lol/champions?position=<lane>` for the tier list (see `crates/provider-opgg/src/flight.rs` for the parser). Runes come from a clean `rune_pages` data prop — the most complete rune support of any provider here (full primary/secondary/shards + summoner spells). Counters and tier list come from a clean `data` prop each (tier list needs the `position` query param — the page's default response ships an unrelated small "trending" preview instead of the full per-lane table). Items and skill max order are scraped out of the rendered element tree (no clean data prop for them). **Matchup-specific rune pages return `NotEnoughData`** — not reachable through what the page ships client-side. The site's CDN (CloudFront + bot detection) 403s requests that look like headless Chrome but accepts a plain HTTP client with a browser `User-Agent`. Champion slug = the Data Dragon alias lowercased, same convention as LoLalytics.
- **`HardcodedProvider`** (`overlay-provider`) — offline fallback with a tiny champion-damage table. Also home to `champion_damage_type`, used by `classify_threats`.

The active source is persisted in `settings.json` as `dataSource` and switchable from the settings panel.

### Overlay window mechanics

Configured in `tauri.conf.json` as two windows:

- `main` is the transparent, decorations-off, always-on-top, click-through, **`focusable: false`** overlay. On startup `lib.rs` resizes it to fill the primary monitor and enables click-through. **LoL must run in Borderless mode** — exclusive fullscreen hides the overlay.
- `control` is a normal focusable window. It starts as the compact status/settings window near the lower-left of the primary monitor, then expands for champ select rune import / OPENLOL UI and returns to compact mode afterward.

**Region-based click-through (`hittest.rs`):** the OS only supports click-through per window, so per-element clickability is emulated: the frontend reports the rects of visible `[data-hit]` elements (`set_hit_regions`), and a ~60 Hz cursor-watcher task flips `set_ignore_cursor_events` only while the cursor is inside one, a drag is held via `set_drag_active`, or forced interactivity is enabled by command.

Global hotkeys (`hotkeys.rs`, desktop only):
- `Ctrl+Shift+O` — show/focus the normal control window.
- `Ctrl+Shift+M` — move the overlay to the next monitor.
- `Ctrl+Shift+D` — toggle debug/mock mode (`mock.rs`): drives the UI with synthetic game state through the real provider pipeline.

## Non-obvious gotchas

- **`reqwest` uses `native-tls`, not rustls, on purpose.** Riot's Live Client server closes the socket without a TLS `close_notify`, which rustls treats as a hard error. Loopback clients set `danger_accept_invalid_certs(true)` for the self-signed cert.
- **DeepLoL quirks:** browser-like `User-Agent` required (403 without it). No `language` query on `/champion/build`. `platform_id` must be numbered (`JP1`, `NA1`, …) except `KR`.
- **`null_default` serde helper** in `overlay-provider-deeplol`: DeepLoL sends explicit `null` for some fields; the helper maps present-null → default. Regression test included.
- **u.gg JSON** uses custom Deserialize visitors (ported from uggo's `ugg-types`); don't hand-roll replacements.
- **Riot ToS:** reading the official APIs and borderless overlay drawing are ToS-compliant. But **writing runes via the LCU requires registering the app with Riot before any public release** (https://developer.riotgames.com/).
