# lol-overlay

軽量 League of Legends オーバーレイ。Tauri (Rust + WebView) と SolidJS で動く。
ゲームプロセスへの注入はせず、Riot のローカル API と公開 CDN データだけを使う。

## 機能

- 試合中のアイテム / スキル順推薦
- チャンピオン選択中の tier list、counter、rune build 表示
- LCU への rune page / summoner spell インポート
- Borderless ゲーム上に重ねる透明・最前面・クリック透過 overlay
- DeepLoL / u.gg / LoLalytics / LOL.PS / OP.GG のBuildデータソース切り替え
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
  provider-lolalytics/        LoLalytics provider
  provider-lolps/             LOL.PS provider (KR / Emerald+ build data)
  provider-opgg/              OP.GG provider
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
bun run build:relay    # Expo Web viewer + Wrangler bundle dry-run
bun run build:mobile   # Expo export
```

最初にルートの`.env.example`を`.env`へコピーする。環境設定はこのルート`.env`だけで
管理し、desktop / mobile / relayは必ずルートの`bun run ...`コマンドから起動する。
RelayはWranglerの`secrets.required`で`MOBILE_RELAY_CREATE_SECRET`だけをプロセス環境から
受け取る。`apps/relay/.dev.vars`は作らない（存在するとWranglerの読み込み規則が変わる）。

| 変数 | 利用先 | 用途 |
| --- | --- | --- |
| `EXPO_PUBLIC_MOBILE_RELAY_URL` | desktop + mobile | Relay URL（ローカルは`http://127.0.0.1:8787`） |
| `MOBILE_RELAY_CREATE_SECRET` | desktop + relay | セッション作成用secret。ローカルでは空でもよい |
| `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` | desktop buildのみ | updater署名 |

モバイルMVPは`bun run dev:relay`、`bun run dev:mobile`、`bun run dev:desktop`の順で
起動する。本番Workerには`cd apps/relay && bunx wrangler secret put
MOBILE_RELAY_CREATE_SECRET`で、desktopと同じsecretを登録する。secretは
`EXPO_PUBLIC_`変数にはせず、frontend bundleへ埋め込まない。
Relay起動後は`/debug`でHono JSXの診断画面、`/viewer/`でiOSと共通のExpo Web画面を
確認できる。`bun run build:relay`とdeployはExpo Web viewerも自動でexportする。

GitHubのリポジトリ設定には次を登録する。配布desktopはGitHub Actionsでビルドされるため、
ローカル`.env`の値はrelease workflowには渡らない。

| GitHub設定 | 種類 | 値 |
| --- | --- | --- |
| `EXPO_PUBLIC_MOBILE_RELAY_URL` | Actions Variable | 本番Relay URL |
| `MOBILE_RELAY_CREATE_SECRET` | Actions Secret | Workerに登録したものと同じ値 |
| `TAURI_SIGNING_PRIVATE_KEY` | Actions Secret | updater署名鍵 |

Relayは1ペアリングにつき1 Durable Objectを作り、4時間で失効する。切断（DISCONNECT）時は
producerがセッションをrevokeし、viewerのWebSocketも閉じる。最新スナップショットだけを
Durable Objectに保持し、履歴は保存しない。6桁コードは専用Durable Objectで10分間保持する。
想定利用はWindows上のデスクトップから試合中データを送ること
（OS自体の強制チェックはない）。公開Workerでは`MOBILE_RELAY_CREATE_SECRET`必須（未設定は503）。
ローカル`wrangler dev`のみsecretなしを許可する。Workerをデプロイする前に
`apps/relay/wrangler.jsonc`の`MOBILE_APP_URL`とWorker名を本番値へ変更する。

通常の検証:

```bash
bun run check
CI=true bun run check
cargo test --workspace --lib
cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture
cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture
cargo test -p overlay-provider-lolps --lib -- --ignored --nocapture
```

## 実行時メモ

- ターゲットは Windows。LCU / Live Client API / click-through の実機確認は
  Windows + League client が必要。
- LoL は Borderless mode で起動する。排他的 fullscreen には重ねられない。
- LCU lockfile の探索と認証は `irelia` が行うため、通常は環境変数不要。
- 外部データは Data Dragon と選択中Build Providerから取得し、短い timeout と retry、
  TTL 付き cache を通す。LOL.PSはKR・Emerald+固定で、ロールだけ現在のpositionに追従する。
- `reference-repo.local/` は gitignore 済みのローカル参照用ディレクトリ。容量が
  大きい場合は内部の Rust `target/` に対して `cargo clean` すれば回収できる。

## Riot ToS

公式 API の読み取りと Borderless window の重ね描画はメモリ読取や注入を伴わない。
ただし rune page 書き込みなど LCU を使うアプリを公開する前には Riot Developer
Portal での登録・審査が必要。
