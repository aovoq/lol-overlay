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
- iPhoneをサブディスプレイとして使うExpoアプリとCloudflare Relay

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
  mobile.rs                   ephemeral relay sessions and snapshot publisher
apps/
  mobile/                     Expo iOS app
  relay/                      Cloudflare Worker + Durable Object
packages/
  protocol/                   relay/mobile shared TypeScript protocol
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
bun install
bun run tauri dev      # Vite dev server + Tauri shell
bun run dev            # frontend only
bun run tauri build    # distributable build, run on Windows
bun run dev:relay      # local Cloudflare Relay on :8787
bun run dev:mobile     # Expo dev server
bun run build:relay    # Wrangler bundle dry-run
bun run build:mobile   # Expo export
```

モバイルMVPをローカルで動かす場合は、`bun run dev:relay`、`bun run dev:mobile`、
`bun run tauri dev`の順で起動する。デスクトップは未設定時にデプロイ済みの
`https://lol-overlay-relay.voq.workers.dev`を使う。ローカルRelayへ接続する場合は
`VITE_MOBILE_RELAY_URL=http://127.0.0.1:8787`を設定する。

Relayは1ペアリングにつき1 Durable Objectを作り、4時間で失効する。
Windowsからのスナップショットだけを受け付け、履歴は保存しない。
Workerをデプロイする前に`apps/relay/wrangler.jsonc`の`MOBILE_APP_URL`と
Worker名を本番値へ変更する。

通常の検証:

```bash
bun run check
CI=true bun run check
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
