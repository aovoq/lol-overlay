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

- [x] **CSP が無効(`csp: null`)+ `withGlobalTauri: true`**(2026-07-05 修正: `withGlobalTauri=false` + 制限 CSP を設定) — `src-tauri/tauri.conf.json:13,51`
  DeepLoL / u.gg / Data Dragon 由来の外部データを描画するのに CSP が無く、全 JS に `window.__TAURI__` が露出。プロバイダ由来文字列のエスケープ漏れ 1 つで Tauri コマンドブリッジ付き XSS になる。→ 制限的な CSP を設定し、`withGlobalTauri` の必要性を再検討。

- [x] **Live Client の HTTP クライアントにタイムアウトが無い**(2026-07-05 修正: 8秒 timeout + 短い retry/backoff を追加) — `crates/live-client/src/lib.rs:101-104`
  他クライアントは 8 秒タイムアウト設定済みだが Live Client だけ未設定。ローディング画面中に `allgamedata` がハングするとポーラーが無期限ブロック。→ `.timeout(...)` を追加。

- [x] **静的キャッシュ(patch / champion / item)が一切失効しない**(2026-07-05 修正: Data Dragon / u.gg / DeepLoL に 6時間 TTL を追加) — `crates/ddragon/src/lib.rs:89-110`, `crates/provider-ugg/src/api.rs:63-91`, `crates/provider-deeplol/src/lib.rs:107-136`
  プロセス生存中ずっと初回ロードのまま。パッチを跨いで起動しっぱなしだと旧パッチのビルドを出し続け、新チャンピオンは「unknown champion」になる。→ TTL またはゲームフロー(ゲーム開始時)起点のリフレッシュ。

- [x] **Mutex 全箇所で `.lock().unwrap()`(ポイズン時に連鎖パニック)**(2026-07-05 修正: Tauri engine state を `parking_lot::Mutex` に置換) — `src-tauri/src/commands.rs`, `src-tauri/src/engine.rs` の各所
  どこかのスレッドがロック保持中にパニックすると以降の全コマンド/ポーリングが巻き添えでパニック。→ `parking_lot::Mutex` への置き換え、または `unwrap_or_else(PoisonError::into_inner)`。

### エラーの黙殺(サイレント劣化)

- [x] **アイテム取得エラーが空リストに化けてログも出ない**(2026-07-05 修正: items/skill fetch 失敗時に warn log を出して UI 継続) — `src-tauri/src/engine.rs:727`
  `provider.items(...).unwrap_or_default()`。ゲーム中の一時的なネットワークエラーで推奨パネルが無言で空になる。同関数内の他フェッチ同様 `log(&app, "warn", ...)` を出す。

- [x] **ネットワーク障害が「unknown champion」として報告される**(2026-07-05 修正: champion id 解決を `Result<Option<_>>` にして初期化エラーを伝播) — `crates/provider-deeplol/src/lib.rs:143-145`
  `ensure_static` の失敗が `champion_id() == None` に潰され、呼び出し側で未知チャンピオン扱い。実エラーは stderr のみ。→ エラーを型で区別して伝播。

- [x] **Live Client の snapshot がパースエラーも「試合中でない」に潰す**(2026-07-05 修正: unavailable と parse error を分離し、poller で初回 warn log) — `crates/live-client/src/lib.rs:122`
  `self.raw().await.ok()?`。API スキーマ変更によるパース破壊が「永遠にゲーム未検出」に見えて気付けない。→ 少なくともパースエラーはログに出す。

- [x] **`eprintln!` によるロギングが release ビルドで不可視**(2026-07-05 修正: Tauri command の失敗ログを `events::log` に統一) — `src-tauri/src/commands.rs:202,219,238`
  `windows_subsystem = "windows"` では stderr が見えない。他所で使っている `events::log` ヘルパーに統一。

- [x] **保存済みデータソース復元の失敗を無視**(2026-07-05 修正: restore 失敗時に warn log を出す) — `src-tauri/src/engine.rs:258`
  `let _ = self.provider.set_active(...)`。復元失敗時にデフォルトへ落ちた理由がログに残らない。

### フロントエンド

- [x] **`interactive` 機能がエンドツーエンドで死んでいる**(2026-07-05 修正: backend signal から body class へ bridge) — `src/state/backend.ts:22,40,69`, `src/app.css:278,298`
  バックエンドは `interactive` イベントを emit し CSS に `body.interactive::after` のリングも定義済みだが、`<body>` に `interactive` クラスを付ける処理がどこにも無く、シグナルもどのコンポーネントも読んでいない。→ signal→body クラスのブリッジを実装するか、経路ごと削除。

- [x] **オーバーレイモードでビルドリストがスクロール不能**(2026-07-05 修正: ScrollArea に `data-hit` bridge を追加) — `src/components/ingame/InGamePanel.tsx:92,193`
  `[data-hit]` が `<header>` にしか無く、`rec-list` の ScrollArea 領域はクリックスルーのままなのでホイールが届かない。→ スクロール領域にも `data-hit` を付与。

- [x] **`SkillCard` の非同期エフェクトにレースあり(古いレスポンスが勝つ)**(2026-07-05 修正: generation guard で stale resolution を無視) — `src/components/ingame/SkillOrder.tsx:31-44`
  チャンピオン切り替え時、先行の `getAbility().then()` が後から解決すると古いアイコン/名前で上書き。→ 世代ガードか AbortController。

- [x] **Counters にエラー状態が無い(失敗が「Not enough data yet」に見える)**(2026-07-05 修正: `SectionError` + Retry UI に統一) — `src/components/openlol/Counters.tsx:39-69`
  `BuildArea`/`TierLists` は `SectionError` + Retry を出すのに Counters だけ握りつぶす。→ 同じエラー UI に揃える。

- [x] **モジュールレベルの `listen()` 9 本に `.catch()` が無い**(2026-07-05 修正: 全 module-level listener に rejection guard を追加) — `src/state/backend.ts:61-71`
  他ファイルは全て `.catch(() => {})` を付けており不整合。reject 時に unhandled rejection になる。

### 保守性 / 重複

- [x] **DeepLoL / u.gg プロバイダ間のロジック重複**(2026-07-05 修正: item/counter/rune flatten/閾値 helper を `overlay-provider` に集約) — `crates/provider-deeplol/src/lib.rs`, `crates/provider-ugg/src/lib.rs`
  アイテム推奨構築ループ(スコア式 `1.0 - i*0.08` と reason 文字列)、カウンター反転ロジック、`runes()` の flatten、role/lane 文字列マッピング、platform→region テーブル(3 箇所)、`MIN_MATCHUP_GAMES = 30`(2 箇所)、RuneBuild 組み立てが両実装にコピーされている。片方だけ直すバグの温床。→ 共有部を `overlay-provider` に集約。

- [x] **`provider-deeplol/src/lib.rs` が 1751 行**(2026-07-05 修正: `types.rs` / `runes.rs` / `tests.rs` に分割し lib.rs を縮小) — レスポンス構造体(~270 行)、集計ヘルパー、テスト(~530 行)をモジュール分割。u.gg 側は既に `api.rs`/`tier_list.rs`/`types/` に分割済みで DeepLoL だけ外れ値。

- [x] **`ProviderProxy::new(initial)` が未登録 kind を検証しない**(2026-07-05 修正: provider map 注入 + initial 検証 + routing test 追加) — `crates/provider/src/proxy.rs:93-99`
  `set_active` はガードがあるのに初期値は素通しで、配線ミス時に初回呼び出しでパニック。→ `new` でも検証。

- [x] **README.md が 2 世代前の構成を記載**(2026-07-05 修正: workspace / SolidJS / provider 構成へ更新) — `README.md:15-35,57-66`
  workspace リファクタ・SolidJS 移行前のフラット構成(`src-tauri/src/lcu.rs`、`src/main.ts` 等)と、「未実装」扱いの provider を「次のステップ」として記載。CLAUDE.md / AGENTS.md は最新なので README を追随させる。

- [x] **`src-tauri/Cargo.lock` が孤児(ルートのロックと重複)**(2026-07-05 修正: 削除して root lockfile に一本化) — workspace 化後の残骸で既に stale。削除する。

### パフォーマンス

- [x] **60Hz カーソルウォッチャーが毎 tick OS/IPC 呼び出し 3 連発**(2026-07-05 修正: window geometry cache + InGame window mode で停止) — `src-tauri/src/hittest.rs:52-92`
  `cursor_position` / `outer_position` / `scale_factor` を毎 tick 取得(後者 2 つはほぼ定数)。InGame モードで main window が隠れていても回り続ける。ゲームと同居するプロセスとしては無駄。→ 定数のキャッシュ+不要モードでの停止。

- [x] **`setInterval(reportHitRegions, 250)` が全 `[data-hit]` に毎回 `getBoundingClientRect()`**(2026-07-05 修正: ResizeObserver/MutationObserver + rAF スケジューリングへ変更) — `src/lib/hitRegions.ts:21-24`
  250ms ごとの強制同期レイアウト。ResizeObserver/MutationObserver ベースか可視状態でのゲートを検討。

### テスト

- [x] **テストゼロのクレート: `overlay-provider`(ProviderProxy ルーティング・`classify_threats`)、`overlay-live-client`、`overlay-types`**(2026-07-05 修正: proxy/threat/live-client/types の unit test を追加) — 中核のルーティング/脅威分類が未テスト。
- [x] **フロントエンドのテストが皆無**(2026-07-05 修正: Vitest 最小構成 + assets/openlol formatter tests を追加) — `package.json` に test スクリプト・vitest 等が無い。最低限 `assets.ts`/`openlol.ts` のフォーマッタと `backend.ts` のイベント配線から。
- [x] **src-tauri の純関数が未テスト**(2026-07-05 修正: rank/window/layout/settings serde の unit test を追加) — `rank_value`, `desired_window_mode`, `clamp_control_layout`, champ-select dedup(`engine.rs`)、`Settings` serde デフォルト。ユニットテストがあるのは `hittest.rs` の 3 本のみ。
- [x] **パニック経路のリグレッションテスト** — 上記 High 2 件(切り詰め overview 配列、0 試合 matchup)のテストを追加済み(2026-07-05)。

## 低優先度

- [x] `crates/provider/src/hardcoded.rs:73-74`(2026-07-05 修正: Zac 重複を除去) — `TANK` テーブルに `"Zac"` が重複。テーブル自体も静的で新チャンピオンは `Unknown` 扱いになる。
- [x] `crates/provider/src/hardcoded.rs`, `threat.rs`(2026-07-05 判断: CLAUDE.md 記載どおり offline fallback / threat classifier として保持し、テスト追加) — `HardcodedProvider` / `classify_threats` は本番で未配線の実質デッドコード。使うか消すか決める。
- [x] `crates/provider-ugg/src/types/arena_overview.rs`(~323 行)(2026-07-05 修正: 到達不能 Arena parser を削除) — Arena パースは到達不能(`is_arena_mode` が常に `NotEnoughData`)。使うまで削除候補。
- [x] `crates/provider-ugg/src/types/mappings.rs:242-253`(2026-07-05 修正: 壊れた `get_region` を削除し `Region::from_str` を exact/alias 化) — 未使用の `get_region` はロジックもバグっている(`"ru"` → `KR` にマッチ)。将来の罠。
- [x] `crates/provider-ugg/src/api.rs:55-61`(2026-07-05 修正: `16.11.1` / `16.11` とも `16_11` に変換するテストへ更新) — `patch_from_ddragon` が 2 セグメント版(`"16.11"` → `"16"`)を誤変換。テストがその誤動作を正として固定化している。
- [x] 全 HTTP クライアントにリトライ/バックオフ無し(2026-07-05 修正: Data Dragon / Live Client / DeepLoL / u.gg に connect/timeout/5xx retry を追加) — DeepLoL は非 KR で `/champion/rank` が 500 を返すことが既知(コメントあり)なのに単発試行。
- [x] `crates/ddragon/src/lib.rs:49`(2026-07-05 修正: `try_new()` で構築エラーを返し、`new()` は panic しない fallback に変更) — クライアント構築失敗で `expect` パニック(他 API は `DdragonError` を返す設計)。
- [x] `src-tauri/src/hotkeys.rs:32-35`(2026-07-05 修正: `Ctrl+Shift+O` は main window 無しでも control window を表示) — `Ctrl+Shift+O` が main window の存在に不要に依存(無くても control window は出せる)。
- [x] `src-tauri/src/hotkeys.rs:76-84`(2026-07-05 修正: mock generation guard で旧 loop の stale cleanup を抑止) — モックホットキー連打で旧ループが最長 1.5 秒二重 emit(デバッグ専用、実害小)。
- [x] `src-tauri/tauri.conf.json:29-30`(2026-07-05 修正: 起動時 main overlay を 1x1 にし、setup で実 monitor bounds へ拡張) — 起動時サイズ 1920×1080 ハードコード。非 1080p/HiDPI で一瞬サイズ違いのフラッシュ。
- [x] `src-tauri/src/engine.rs:47-48` + `commands.rs:48-54`(2026-07-05 修正: legacy `pinned` / `champselect_window` / `set_pinned` を削除) — legacy の `pinned` / `champselect_window` と、誰も読まない値を書く `set_pinned` コマンドが残存。
- [x] `src/components/ingame/InGamePanel.tsx:38-43`, `src/lib/drag.ts:85`(2026-07-05 修正: transition/pointer listener cleanup を追加) — `transitionend` / `pointerdown` リスナーに `onCleanup` が無く、`<Show>` 配下のマウント/アンマウント繰り返しでリーク。
- [x] `src/state/settings.ts:83-95`(2026-07-05 修正: `data-source` event を購読して dropdown state を同期) — `data-source` イベントを購読していないため、外部起点でソースが変わると設定 UI のドロップダウンが stale。
- [x] `src/app.css:286-296`(2026-07-05 修正: 存在しない selector の dead CSS を削除) — `body.champselect .status-chip` 等、対象クラスが存在しないデッド CSS。
- [x] `src/components/openlol/Tabs.tsx:84`(2026-07-05 修正: dropdown を relative container 配下に置き、固定 left を削除) — ドロップダウン位置 `left-[140px]` ハードコード。タブ幅変更で崩れる。
- [x] `src-tauri/Cargo.toml:4-5`(2026-07-05 修正: description/authors を実アプリ用 metadata に更新) — `description = "A Tauri App"` / `authors = ["you"]` がスキャフォールドのまま(バンドルメタデータに漏れる)。
- [x] `reference-repo.local/` が 813MB(大半は内部の `target/`)(2026-07-05 修正: README に `cargo clean` で回収可能な旨を追記) — gitignore 済みで問題は無いが、`cargo clean` で回収可能な旨を docs に一言。

## リリース前の必須事項(コード外)

- [ ] **Riot へのアプリ登録** — ルーンページ書き込み(`/lol-perks/*`)を含むため、公開リリース前に https://developer.riotgames.com/ での登録が必須(CLAUDE.md 記載)。
- [ ] **Windows 実機検証** — hittest(クリックスルー領域)、SolidJS 移行後 UI、workspace リファクタ後の動作確認が未了(LCU / Live Client API は Windows + 実クライアントが必要)。
