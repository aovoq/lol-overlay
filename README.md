# lol-overlay

軽量 League of Legends オーバーレイ。Tauri (Rust + WebView) と SolidJS で動く。
ゲームプロセスへの注入はせず、Riot のローカル API と公開 CDN データだけを使う。

## 機能

- 試合中のアイテム / スキル順推薦
- チャンピオン選択中の tier list、counter、rune build 表示
- LCU への rune page / summoner spell インポート
- Borderless ゲーム上に重ねる透明・最前面・クリック透過 overlay
- DeepLoL / u.gg のデータソース切り替え
- macOS でも UI を触れる debug mock mode (`Ctrl+Shift+D`)

## 構成

```text
src/                         SolidJS frontend
  components/                 overlay / OPENLOL / settings UI
  state/                      Tauri event listeners and caches
  lib/                        drag, hit-region, OPENLOL helpers
src-tauri/src/                Tauri app shell
  engine.rs                   shared state, poller, rune processor, window modes
  commands.rs                 frontend invoke commands
  events.rs                   camelCase event payloads
  hittest.rs                  data-hit based click-through control
  hotkeys.rs                  global shortcuts
  mock.rs                     local synthetic scenarios
crates/
  lcu/                        League Client Update API and WebSocket helpers
  live-client/                Live Client Data API client
  ddragon/                    Data Dragon static maps
  provider/                   BuildProvider trait, proxy, shared helpers
  provider-deeplol/           DeepLoL provider
  provider-ugg/               u.gg provider
  types/                      shared serde payloads mirrored by src/types.ts
```

Backend serde payloads use `camelCase`; keep `src/types.ts` in sync when event or
command payloads change. `reqwest` intentionally uses native TLS because Riot's
Live Client API does not close TLS in a rustls-friendly way.

## 開発

```bash
pnpm install
pnpm tauri dev      # Vite dev server + Tauri shell
pnpm dev            # frontend only
pnpm tauri build    # distributable build, run on Windows
```

通常の検証:

```bash
pnpm check
CI=true pnpm check
cargo test --workspace --lib
cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture
cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
```

## 実行時メモ

- ターゲットは Windows。LCU / Live Client API / click-through の実機確認は
  Windows + League client が必要。
- LoL は Borderless mode で起動する。排他的 fullscreen には重ねられない。
- LCU lockfile の探索と認証は `irelia` が行うため、通常は環境変数不要。
- 外部データは Data Dragon / DeepLoL / u.gg から取得し、短い timeout と retry、
  TTL 付き cache を通す。
- `reference-repo.local/` は gitignore 済みのローカル参照用ディレクトリ。容量が
  大きい場合は内部の Rust `target/` に対して `cargo clean` すれば回収できる。

## Riot ToS

公式 API の読み取りと Borderless window の重ね描画はメモリ読取や注入を伴わない。
ただし rune page 書き込みなど LCU を使うアプリを公開する前には Riot Developer
Portal での登録・審査が必要。
