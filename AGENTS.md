# AGENTS.md

Guidance for coding agents working in this repository.

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

cargo test --workspace --lib
cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture
cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
```

## Formatting and linting

- Frontend formatting/linting uses **Biome** (`biome.json`) for TS/TSX/CSS/JSON/HTML.
- Type checking is still **TypeScript** (`pnpm typecheck`); Biome does not replace `tsc`.
- Rust formatting uses **rustfmt** with LF newlines (`rustfmt.toml`).
- Rust linting uses **Clippy** via `cargo clippy --workspace --all-targets -- -D warnings`.
- TOML formatting uses **Taplo** (`.taplo.toml`) for the workspace `Cargo.toml` files.
- Run `pnpm check` before handing off a non-trivial change.
- Use `pnpm format` to apply formatter changes instead of hand-formatting large diffs.
- `.gitattributes` pins text files to LF. Do not introduce CRLF-only churn.

## Project notes

- This is a Tauri app: Rust backend, SolidJS frontend.
- Target platform is Windows. Anything touching LCU or Live Client Data API needs a running League client on Windows.
- Shared Rust logic lives in the Cargo workspace under `crates/`; the app shell is in `src-tauri/`.
- Frontend event payloads mirror Rust serde structs and use camelCase.
