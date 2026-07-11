# AGENTS.md

Guidance for coding agents working in this repository.

## Commands

```bash
bun install
bun run tauri dev      # run the app (Vite dev server + Tauri)
bun run tauri build    # distributable build (run on Windows)

bun run dev            # frontend only (Vite, no Tauri shell)
bun run build          # tsc typecheck + vite build
bun run format         # Biome + rustfmt + Taplo write fixes
bun run format:check   # check TS/CSS/JSON, Rust, and TOML formatting
bun run lint           # Biome lint + strict Clippy
bun run check          # format check + lint + typecheck + Rust unit tests

cargo test --workspace --lib
cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture
cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
```

## Formatting and linting

- Frontend formatting/linting uses **Biome** (`biome.json`) for TS/TSX/CSS/JSON/HTML.
- Type checking is still **TypeScript** (`bun run typecheck`); Biome does not replace `tsc`.
- Rust formatting uses **rustfmt** with LF newlines (`rustfmt.toml`).
- Rust linting uses **Clippy** via `cargo clippy --workspace --all-targets -- -D warnings`.
- TOML formatting uses **Taplo** (`.taplo.toml`) for the workspace `Cargo.toml` files.
- Run `bun run check` before handing off a non-trivial change.
- Use `bun run format` to apply formatter changes instead of hand-formatting large diffs.
- `.gitattributes` pins text files to LF. Do not introduce CRLF-only churn.

## Project notes

- This is a Tauri app: Rust backend, SolidJS frontend.
- Target platform is Windows. Anything touching LCU or Live Client Data API needs a running League client on Windows.
- Shared Rust logic lives in the Cargo workspace under `crates/`; the app shell is in `src-tauri/`.
- Frontend event payloads mirror Rust serde structs and use camelCase.
- Mobile sideboard: `apps/mobile` (Expo) + `apps/relay` (Cloudflare Worker/DO) + `packages/protocol` + `src-tauri/src/mobile.rs`. Pairing uses `VITE_MOBILE_RELAY_URL`; optional `MOBILE_RELAY_CREATE_SECRET` / Worker `SESSION_CREATE_SECRET`.
