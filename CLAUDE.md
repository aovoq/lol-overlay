# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A lightweight League of Legends overlay built with Tauri (Rust backend + WebView frontend). It never injects into or reads the game's memory — it only reads Riot's official local APIs and draws a transparent, always-on-top, click-through window over the (borderless) game. Two features: in-game item recommendations and automatic rune-page import during champ select.

## Commands

```bash
pnpm install
pnpm tauri dev      # run the app (Vite dev server + Tauri)
pnpm tauri build    # distributable build (run on Windows)

pnpm dev            # frontend only (Vite, no Tauri shell)
pnpm build          # tsc typecheck + vite build

# Rust tests (run from src-tauri/)
cargo test --lib
# Network-dependent end-to-end check against the live DeepLoL + Data Dragon APIs:
cargo test --lib provider::deeplol -- --ignored --nocapture
```

**Target platform is Windows.** UI/frontend work runs fine on Mac, but anything touching the LCU or Live Client Data API requires a running LoL client (Windows). There is no JS test runner and no linter beyond `tsc` (strict mode, `noUnusedLocals`/`noUnusedParameters`).

## Architecture

The Rust backend (`src-tauri/src/`) polls/subscribes to Riot's two local APIs, runs recommendations through a pluggable data provider, and emits Tauri events. The frontend (`src/main.ts`, plain TypeScript — no framework) listens for those events and renders panels into `index.html`.

### Two Riot data sources

- **LCU API** (`lcu.rs`) — the League *client* (lobby, champ select, runes). Accessed via the [`irelia`](https://github.com/AlsoSylv/Irelia) crate, which handles lockfile discovery, auth, and the self-signed cert, so no env vars or config are needed. Used for gameflow phase, champ-select session, and writing rune pages (`/lol-perks/*`).
- **Live Client Data API** (`live_client.rs`) — read-only REST on `https://127.0.0.1:2999` while a *match* is running. No auth, self-signed cert. Has **no WebSocket**, so in-game state is polled.

### Engine orchestration (`engine.rs`, wired up in `lib.rs::run`)

Three concurrent pieces share the channel `tokio::sync::mpsc`:

1. **WebSocket subscriber** (`lcu::subscribe_champ_select`) pushes champ-select session updates onto the channel as they happen. The handle is `mem::forget`'d to keep it alive for the app's lifetime.
2. **`rune_processor`** drains the channel and, on a *pick change* (deduped by champion id), looks up runes via the provider and writes them through the LCU. The same dedup makes duplicate WS + REST sessions harmless.
3. **`poller`** (every 2s) tracks gameflow phase + in-game state for the UI, AND re-pushes the champ-select session onto the channel as a REST fallback in case the WebSocket missed an update. In-game item recommendations are produced here (Live Client API has no WS).

### Backend → frontend events (Tauri `emit` / `listen`)

`phase`, `recommendations`, `rune-imported`, `log`, `interactive`. Payload structs live in `events.rs` and are mirrored as TypeScript interfaces in `src/main.ts`. **All payloads use `#[serde(rename_all = "camelCase")]`** so the Rust field names match the TS interfaces — keep both sides in sync when changing a payload.

Frontend → backend commands (`commands.rs`): settings/layout setters (`get_settings`, `set_auto_import`, `set_ingame_collapsed`, …), the HEXGATE data lookups (`get_tier_list`, `get_rune_build`, `get_counters`, `import_build`), and the click-through plumbing (`set_hit_regions`, `set_drag_active`, `set_interactive`).

### Data provider abstraction (`provider/`)

Everything the overlay needs "from the internet" flows through the `BuildProvider` trait (`items()` + `runes()`). Swapping data sources = one more `impl BuildProvider` + one line in `lib.rs::run`.

- **`deeplol.rs`** (`DeepLolProvider`) — the active provider. Pulls real meta builds/runes from DeepLoL's CDN API and champion/item name maps from Data Dragon. Caches static maps + per-champion builds for the process lifetime (the poller calls `items()` every ~2s, so it must not hit the network each time).
- **`hardcoded.rs`** (`HardcodedProvider`) — offline fallback with a tiny built-in champion-damage table. Also home to `champion_damage_type`, used by the shared `classify_threats` heuristic (counts AD/AP/tank on the enemy team).

### Overlay window mechanics

Configured in `tauri.conf.json` (transparent, decorations-off, always-on-top, click-through, **`focusable: false`** so overlay clicks never steal keyboard focus from the game, `macOSPrivateApi`). On startup `lib.rs` resizes the window to fill the primary monitor and enables click-through. **LoL must run in Borderless mode** — exclusive fullscreen hides the overlay.

**Region-based click-through (`hittest.rs`):** the OS only supports click-through per window, so per-element clickability is emulated: the frontend reports the rects of visible `[data-hit]` elements (`set_hit_regions`, refreshed on a 250 ms interval in `main.ts`), and a ~60 Hz cursor-watcher task flips `set_ignore_cursor_events` only while the cursor is inside one (or a drag is held via `set_drag_active`, or the Ctrl+Shift+O override is on). Hit regions: the in-game panel header (drag to move, chevron to collapse — persisted in `UiLayout`), the whole HEXGATE champ-select panel (hover previews need mouse input; its header drags the window and has a settings gear), and the settings panel. `apply_window_mode` resets the override and the watcher's applied-state cache on every mode switch.

Global hotkeys (`hotkeys.rs`, desktop only):
- `Ctrl+Shift+O` — emergency override: whole-window interactive (shows the settings panel). Normal operation never needs it — headers are always clickable.
- `Ctrl+Shift+M` — move the overlay to the next monitor.
- `Ctrl+Shift+D` — toggle debug/mock mode (`mock.rs`): drives the UI with a synthetic game state through the *real* provider pipeline, so you can develop the overlay without launching League. The poller pauses while mock mode is on.

## Non-obvious gotchas

- **`reqwest` uses `native-tls`, not rustls, on purpose.** Riot's Live Client server closes the socket without a TLS `close_notify`, which rustls treats as a hard error. Both HTTP clients also set `danger_accept_invalid_certs(true)` for the loopback self-signed cert.
- **DeepLoL quirks:** its CDN 403s requests with no `User-Agent`, so the client sends a browser-like one. The `language` query param is deliberately omitted from `/champion/build` (DeepLoL returns an empty body if present). `platform_id` must be a *numbered* region (`JP1`, `NA1`, …) except `KR`.
- **`null_default` serde helper (`deeplol.rs`)**: DeepLoL sends explicit `null` for some fields (e.g. an Aram lane's `games`), which a plain `i64`/`f64` field rejects and aborts the whole parse. The helper maps present-`null` → `T::default()`; it's applied to every deserialized field. There's a regression test for this.
- **Riot ToS:** reading the official APIs and borderless overlay drawing are ToS-compliant. But **writing runes via the LCU requires registering the app with Riot before any public release** (https://developer.riotgames.com/).
