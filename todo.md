# TODO — プロジェクト問題点リスト

2026-07-05 時点のコードレビューで洗い出した問題点。`pnpm check`(format / lint / typecheck / Rust unit tests)は現状すべてパスしており、以下は静的チェックでは捕捉できない問題。

## 高優先度

- [x] **LCU WebSocket 購読の失敗が握りつぶされ、再試行されない**(2026-07-05 修正: 購読ガード `ChampSelectSubscription` + 3秒間隔リトライ + 停止検知) — `crates/lcu/src/ws.rs:25-29`, `src-tauri/src/lib.rs:91-92`
  `let _ = ws.subscribe(...)` でエラーを破棄し常に `Ok(())` を返すため、League クライアント起動前にオーバーレイを立ち上げる(よくある順序)と購読が静かに失敗したまま二度と再接続されない。`lib.rs` 側の `.expect()` は実質デッドコード。チャンプセレクト検知が 2 秒ポーリングのフォールバック頼みになり、WS の低レイテンシ経路が事実上死ぬ。LCU 再起動時の再接続も無い。→ 購読失敗を返す/リトライループを入れる。

- [x] **u.gg overview パース時の配列直接インデックスでパニックし得る**(2026-07-05 修正: `.get()` + serde error 化、切り詰め配列テスト追加) — `crates/provider-ugg/src/types/default_overview.rs:335-362`
  `late_items[0..2]`、`match_info[0]`/`match_info[1]` を長さ検証なしで直接インデックスしている(`match_info` は `unwrap_or_default()` で空 Vec になり得る)。u.gg が切り詰められたレスポンスを返すと deserialize 中にパニック。すぐ上の `low_sample_size` は `.get(1)` を使っており不整合。→ `.get()` + エラー化し、切り詰め配列のリグレッションテストを追加。

- [x] **matchup パースの NaN winrate で `partial_cmp().unwrap()` がパニックし得る**(2026-07-05 修正: 0試合行を除外 + `total_cmp`、回帰テスト追加) — `crates/provider-ugg/src/types/matchups.rs:90-110`
  `matches == 0` の行は `winrate = NaN` になり、`total_matches == 0` のときフィルタ閾値も 0 で NaN 行が生き残る。`sort_by(...partial_cmp().unwrap())` がパニック。→ `total_cmp` を使うか 0 試合行を除外し、テストを追加。

- [ ] **LICENSE ファイルが無い** — リポジトリルート、`package.json`(license フィールド無し)、`src-tauri/Cargo.toml`(license 無し)
  現状「All rights reserved」相当で再利用・コントリビュート不可。公開意図があるならライセンス選定が先決(ルーン書き込みには Riot へのアプリ登録も必要 — CLAUDE.md 参照)。

## 中優先度

### セキュリティ / 堅牢性

- [ ] **CSP が無効(`csp: null`)+ `withGlobalTauri: true`** — `src-tauri/tauri.conf.json:13,51`
  DeepLoL / u.gg / Data Dragon 由来の外部データを描画するのに CSP が無く、全 JS に `window.__TAURI__` が露出。プロバイダ由来文字列のエスケープ漏れ 1 つで Tauri コマンドブリッジ付き XSS になる。→ 制限的な CSP を設定し、`withGlobalTauri` の必要性を再検討。

- [ ] **Live Client の HTTP クライアントにタイムアウトが無い** — `crates/live-client/src/lib.rs:101-104`
  他クライアントは 8 秒タイムアウト設定済みだが Live Client だけ未設定。ローディング画面中に `allgamedata` がハングするとポーラーが無期限ブロック。→ `.timeout(...)` を追加。

- [ ] **静的キャッシュ(patch / champion / item)が一切失効しない** — `crates/ddragon/src/lib.rs:89-110`, `crates/provider-ugg/src/api.rs:63-91`, `crates/provider-deeplol/src/lib.rs:107-136`
  プロセス生存中ずっと初回ロードのまま。パッチを跨いで起動しっぱなしだと旧パッチのビルドを出し続け、新チャンピオンは「unknown champion」になる。→ TTL またはゲームフロー(ゲーム開始時)起点のリフレッシュ。

- [ ] **Mutex 全箇所で `.lock().unwrap()`(ポイズン時に連鎖パニック)** — `src-tauri/src/commands.rs`, `src-tauri/src/engine.rs` の各所
  どこかのスレッドがロック保持中にパニックすると以降の全コマンド/ポーリングが巻き添えでパニック。→ `parking_lot::Mutex` への置き換え、または `unwrap_or_else(PoisonError::into_inner)`。

### エラーの黙殺(サイレント劣化)

- [ ] **アイテム取得エラーが空リストに化けてログも出ない** — `src-tauri/src/engine.rs:727`
  `provider.items(...).unwrap_or_default()`。ゲーム中の一時的なネットワークエラーで推奨パネルが無言で空になる。同関数内の他フェッチ同様 `log(&app, "warn", ...)` を出す。

- [ ] **ネットワーク障害が「unknown champion」として報告される** — `crates/provider-deeplol/src/lib.rs:143-145`
  `ensure_static` の失敗が `champion_id() == None` に潰され、呼び出し側で未知チャンピオン扱い。実エラーは stderr のみ。→ エラーを型で区別して伝播。

- [ ] **Live Client の snapshot がパースエラーも「試合中でない」に潰す** — `crates/live-client/src/lib.rs:122`
  `self.raw().await.ok()?`。API スキーマ変更によるパース破壊が「永遠にゲーム未検出」に見えて気付けない。→ 少なくともパースエラーはログに出す。

- [ ] **`eprintln!` によるロギングが release ビルドで不可視** — `src-tauri/src/commands.rs:202,219,238`
  `windows_subsystem = "windows"` では stderr が見えない。他所で使っている `events::log` ヘルパーに統一。

- [ ] **保存済みデータソース復元の失敗を無視** — `src-tauri/src/engine.rs:258`
  `let _ = self.provider.set_active(...)`。復元失敗時にデフォルトへ落ちた理由がログに残らない。

### フロントエンド

- [ ] **`interactive` 機能がエンドツーエンドで死んでいる** — `src/state/backend.ts:22,40,69`, `src/app.css:278,298`
  バックエンドは `interactive` イベントを emit し CSS に `body.interactive::after` のリングも定義済みだが、`<body>` に `interactive` クラスを付ける処理がどこにも無く、シグナルもどのコンポーネントも読んでいない。→ signal→body クラスのブリッジを実装するか、経路ごと削除。

- [ ] **オーバーレイモードでビルドリストがスクロール不能** — `src/components/ingame/InGamePanel.tsx:92,193`
  `[data-hit]` が `<header>` にしか無く、`rec-list` の ScrollArea 領域はクリックスルーのままなのでホイールが届かない。→ スクロール領域にも `data-hit` を付与。

- [ ] **`SkillCard` の非同期エフェクトにレースあり(古いレスポンスが勝つ)** — `src/components/ingame/SkillOrder.tsx:31-44`
  チャンピオン切り替え時、先行の `getAbility().then()` が後から解決すると古いアイコン/名前で上書き。→ 世代ガードか AbortController。

- [ ] **Counters にエラー状態が無い(失敗が「Not enough data yet」に見える)** — `src/components/openlol/Counters.tsx:39-69`
  `BuildArea`/`TierLists` は `SectionError` + Retry を出すのに Counters だけ握りつぶす。→ 同じエラー UI に揃える。

- [ ] **モジュールレベルの `listen()` 9 本に `.catch()` が無い** — `src/state/backend.ts:61-71`
  他ファイルは全て `.catch(() => {})` を付けており不整合。reject 時に unhandled rejection になる。

### 保守性 / 重複

- [ ] **DeepLoL / u.gg プロバイダ間のロジック重複** — `crates/provider-deeplol/src/lib.rs`, `crates/provider-ugg/src/lib.rs`
  アイテム推奨構築ループ(スコア式 `1.0 - i*0.08` と reason 文字列)、カウンター反転ロジック、`runes()` の flatten、role/lane 文字列マッピング、platform→region テーブル(3 箇所)、`MIN_MATCHUP_GAMES = 30`(2 箇所)、RuneBuild 組み立てが両実装にコピーされている。片方だけ直すバグの温床。→ 共有部を `overlay-provider` に集約。

- [ ] **`provider-deeplol/src/lib.rs` が 1751 行** — レスポンス構造体(~270 行)、集計ヘルパー、テスト(~530 行)をモジュール分割。u.gg 側は既に `api.rs`/`tier_list.rs`/`types/` に分割済みで DeepLoL だけ外れ値。

- [ ] **`ProviderProxy::new(initial)` が未登録 kind を検証しない** — `crates/provider/src/proxy.rs:93-99`
  `set_active` はガードがあるのに初期値は素通しで、配線ミス時に初回呼び出しでパニック。→ `new` でも検証。

- [ ] **README.md が 2 世代前の構成を記載** — `README.md:15-35,57-66`
  workspace リファクタ・SolidJS 移行前のフラット構成(`src-tauri/src/lcu.rs`、`src/main.ts` 等)と、「未実装」扱いの provider を「次のステップ」として記載。CLAUDE.md / AGENTS.md は最新なので README を追随させる。

- [ ] **`src-tauri/Cargo.lock` が孤児(ルートのロックと重複)** — workspace 化後の残骸で既に stale。削除する。

### パフォーマンス

- [ ] **60Hz カーソルウォッチャーが毎 tick OS/IPC 呼び出し 3 連発** — `src-tauri/src/hittest.rs:52-92`
  `cursor_position` / `outer_position` / `scale_factor` を毎 tick 取得(後者 2 つはほぼ定数)。InGame モードで main window が隠れていても回り続ける。ゲームと同居するプロセスとしては無駄。→ 定数のキャッシュ+不要モードでの停止。

- [ ] **`setInterval(reportHitRegions, 250)` が全 `[data-hit]` に毎回 `getBoundingClientRect()`** — `src/lib/hitRegions.ts:21-24`
  250ms ごとの強制同期レイアウト。ResizeObserver/MutationObserver ベースか可視状態でのゲートを検討。

### テスト

- [ ] **テストゼロのクレート: `overlay-provider`(ProviderProxy ルーティング・`classify_threats`)、`overlay-live-client`、`overlay-types`** — 中核のルーティング/脅威分類が未テスト。
- [ ] **フロントエンドのテストが皆無** — `package.json` に test スクリプト・vitest 等が無い。最低限 `assets.ts`/`openlol.ts` のフォーマッタと `backend.ts` のイベント配線から。
- [ ] **src-tauri の純関数が未テスト** — `rank_value`, `desired_window_mode`, `clamp_control_layout`, champ-select dedup(`engine.rs`)、`Settings` serde デフォルト。ユニットテストがあるのは `hittest.rs` の 3 本のみ。
- [x] **パニック経路のリグレッションテスト** — 上記 High 2 件(切り詰め overview 配列、0 試合 matchup)のテストを追加済み(2026-07-05)。

## 低優先度

- [ ] `crates/provider/src/hardcoded.rs:73-74` — `TANK` テーブルに `"Zac"` が重複。テーブル自体も静的で新チャンピオンは `Unknown` 扱いになる。
- [ ] `crates/provider/src/hardcoded.rs`, `threat.rs` — `HardcodedProvider` / `classify_threats` は本番で未配線の実質デッドコード。使うか消すか決める。
- [ ] `crates/provider-ugg/src/types/arena_overview.rs`(~323 行)— Arena パースは到達不能(`is_arena_mode` が常に `NotEnoughData`)。使うまで削除候補。
- [ ] `crates/provider-ugg/src/types/mappings.rs:242-253` — 未使用の `get_region` はロジックもバグっている(`"ru"` → `KR` にマッチ)。将来の罠。
- [ ] `crates/provider-ugg/src/api.rs:55-61` — `patch_from_ddragon` が 2 セグメント版(`"16.11"` → `"16"`)を誤変換。テストがその誤動作を正として固定化している。
- [ ] 全 HTTP クライアントにリトライ/バックオフ無し — DeepLoL は非 KR で `/champion/rank` が 500 を返すことが既知(コメントあり)なのに単発試行。
- [ ] `crates/ddragon/src/lib.rs:49` — クライアント構築失敗で `expect` パニック(他 API は `DdragonError` を返す設計)。
- [ ] `src-tauri/src/hotkeys.rs:32-35` — `Ctrl+Shift+O` が main window の存在に不要に依存(無くても control window は出せる)。
- [ ] `src-tauri/src/hotkeys.rs:76-84` — モックホットキー連打で旧ループが最長 1.5 秒二重 emit(デバッグ専用、実害小)。
- [ ] `src-tauri/tauri.conf.json:29-30` — 起動時サイズ 1920×1080 ハードコード。非 1080p/HiDPI で一瞬サイズ違いのフラッシュ。
- [ ] `src-tauri/src/engine.rs:47-48` + `commands.rs:48-54` — legacy の `pinned` / `champselect_window` と、誰も読まない値を書く `set_pinned` コマンドが残存。
- [ ] `src/components/ingame/InGamePanel.tsx:38-43`, `src/lib/drag.ts:85` — `transitionend` / `pointerdown` リスナーに `onCleanup` が無く、`<Show>` 配下のマウント/アンマウント繰り返しでリーク。
- [ ] `src/state/settings.ts:83-95` — `data-source` イベントを購読していないため、外部起点でソースが変わると設定 UI のドロップダウンが stale。
- [ ] `src/app.css:286-296` — `body.champselect .status-chip` 等、対象クラスが存在しないデッド CSS。
- [ ] `src/components/openlol/Tabs.tsx:84` — ドロップダウン位置 `left-[140px]` ハードコード。タブ幅変更で崩れる。
- [ ] `src-tauri/Cargo.toml:4-5` — `description = "A Tauri App"` / `authors = ["you"]` がスキャフォールドのまま(バンドルメタデータに漏れる)。
- [ ] `reference-repo.local/` が 813MB(大半は内部の `target/`)— gitignore 済みで問題は無いが、`cargo clean` で回収可能な旨を docs に一言。

## リリース前の必須事項(コード外)

- [ ] **Riot へのアプリ登録** — ルーンページ書き込み(`/lol-perks/*`)を含むため、公開リリース前に https://developer.riotgames.com/ での登録が必須(CLAUDE.md 記載)。
- [ ] **Windows 実機検証** — hittest(クリックスルー領域)、SolidJS 移行後 UI、workspace リファクタ後の動作確認が未了(LCU / Live Client API は Windows + 実クライアントが必要)。
