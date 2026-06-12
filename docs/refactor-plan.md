# リファクタリング計画書: Cargo workspace 化 + irelia ラッパー + 統計プロバイダのプロキシ化

実装モデル(composer 2.5)への指示書。この計画はフェーズ単位で完結するように書かれている。
**フェーズを 1 つずつ実装し、各フェーズ末尾の「検証」がすべて通ってから次へ進むこと。**
フェーズごとに 1 コミット。テストが赤いまま次のフェーズへ進んではならない。

- 既存コードの移動が中心。**ロジックの書き換え・最適化・リフォーマットは、本書が明示した箇所以外では禁止**(import 修正・可視性 `pub` 化・パス変更は可)。
- コード内コメントは既存どおり英語で書く。
- `reference-repo.local/` は参考実装(uggo: u.gg の TUI クライアント)。`.gitignore` の `*.local` で**追跡外**なので、読んで移植するのは可だが、**path 依存に追加することと中身を変更することは禁止**。

---

## 1. ゴール / 非ゴール

### ゴール

1. `src-tauri/src/` の単一クレートを、`reference-repo.local`(uggo)と同様の **Cargo workspace + 責務別クレート構成**に再編する。
2. **irelia を専用クレートでラップ**し、`irelia::` 型がそのクレートの外に漏れない構造にする。
3. 統計データ源(現 DeepLoL)を **`ProviderProxy` 経由で実行時に切り替え可能**にし、**u.gg プロバイダを新規実装**する。将来の追加(OP.GG 等)は「`BuildProvider` impl を 1 クレート追加 + 登録 1 行」で済む形にする。
4. 設定 UI からデータソースを切り替えられるようにする(設定は `settings.json` に永続化)。

### 非ゴール

- フロントエンド(SolidJS)の構造変更。変更は「データソース切替 UI」と「切替時のキャッシュクリア」の最小限のみ。
- オーバーレイのウィンドウ機構(hittest / hotkeys / window mode)の変更。これらはアプリクレートに残す。
- 推薦ロジック・集計ロジックの改善。挙動は現状維持。
- u.gg の tier list 対応(v1 では `NotEnoughData` を返す。§7.4 参照)。

---

## 2. 現状と目標構成

### 現状(before)

```
lol-overlay/
├── src/                      # SolidJS frontend
└── src-tauri/
    ├── Cargo.toml            # 単一クレート lol-overlay
    └── src/
        ├── lib.rs main.rs    # 配線・エントリ
        ├── engine.rs         # オーケストレーション(556行)
        ├── commands.rs       # Tauri commands(250行)
        ├── events.rs         # イベント payload 構造体
        ├── error.rs          # アプリ Error
        ├── lcu.rs            # irelia 直接使用(850行)
        ├── live_client.rs    # Live Client Data API
        ├── hittest.rs hotkeys.rs mock.rs
        └── provider/
            ├── mod.rs        # BuildProvider trait + 共有型
            ├── deeplol.rs    # DeepLoL + Data Dragon(1772行)
            └── hardcoded.rs  # オフライン fallback
```

### 目標(after)

```
lol-overlay/
├── Cargo.toml                      # [workspace] ルート(新規)
├── src/                            # frontend(ほぼ不変)
├── crates/
│   ├── types/                      # overlay-types: 純粋データ型(serde のみ)
│   ├── ddragon/                    # overlay-ddragon: Data Dragon クライアント+キャッシュ
│   ├── lcu/                        # overlay-lcu: irelia ラッパー(irelia 依存はここだけ)
│   ├── live-client/                # overlay-live-client: Live Client Data API
│   ├── provider/                   # overlay-provider: trait + ProviderProxy + threat + hardcoded
│   ├── provider-deeplol/           # overlay-provider-deeplol
│   └── provider-ugg/               # overlay-provider-ugg(新規実装)
└── src-tauri/                      # lol-overlay(Tauri アプリ): engine/commands/events/
                                    #   error/hittest/hotkeys/mock/lib/main のみ残す
```

### クレート依存グラフ(上が依存する側)

```
lol-overlay (src-tauri)
 ├── overlay-types
 ├── overlay-lcu ──────────── overlay-types     (irelia はここに閉じる)
 ├── overlay-live-client ──── overlay-types
 ├── overlay-provider ─────── overlay-types     (trait / proxy / hardcoded)
 ├── overlay-provider-deeplol ┬─ overlay-provider, overlay-types, overlay-ddragon
 └── overlay-provider-ugg ────┴─ overlay-provider, overlay-types, overlay-ddragon
```

- `overlay-types` は **serde 以外の依存を持たない**(uggo の `ugg-types` と同じ思想)。
- 矢印の逆流(provider → lcu 等)は禁止。

---

## 3. 不変条件(全フェーズ共通・違反したら即修正)

フロントエンドや Riot API との契約。**1 つでも破ると UI が静かに壊れる。**

1. **Tauri イベント名と payload 形状を変えない**: `phase` / `champ-select` / `recommendations` / `summoner` / `match-history` / `lp-change` / `rune-imported` / `window-mode` / `interactive` / `log`。全 payload は `#[serde(rename_all = "camelCase")]` で `src/types.ts` の TS interface と一致している。型を移動しても serde 属性とフィールドは一切変えない。
2. **Tauri command 名と引数名を変えない**。追加は可(§8 の `get_data_source` 等)。
3. **エラー文字列 `"not-enough-data"`** はフロントの `SectionError` が文字列一致で判定している。アプリの `error.rs::Error::NotEnoughData` の Serialize 結果がこの literal のまま維持されること。
4. **reqwest は native-tls のまま**(rustls 禁止。Riot のローカルサーバは TLS close_notify を送らず rustls では hard error)。ループバックの 2 クライアント(LCU は irelia 内、Live Client は自前)とも `danger_accept_invalid_certs(true)` 維持。
5. **DeepLoL の儀式**: ブラウザ風 `User-Agent` 必須(無いと 403)/ `/champion/build` に `language` クエリを付けない(付けると空ボディ)/ `platform_id` は `KR` 以外は番号付きリージョン(`JP1`, `NA1`, …)。
6. **`null_default` serde ヘルパは deeplol の全 DTO フィールドに適用されたまま**移動する。回帰テストも一緒に移動。
7. **WebSocket ハンドルの keep-alive**: `subscribe_champ_select` のハンドルはアプリ生存中ずっと生きている必要がある(現在 `mem::forget`)。ラップ後もこの意味論を維持(§6.2)。
8. **`items()` は poller から約 2 秒ごとに呼ばれる**。プロバイダはプロセス生涯キャッシュを持ち、毎回ネットワークに行ってはならない(deeplol の既存キャッシュ戦略を維持、ugg も同等に実装)。
9. `tauri.conf.json` は触らない(`focusable: false`、transparent、click-through 等はウィンドウ仕様)。
10. `settings.json` の後方互換: 新フィールドは必ず `#[serde(default)]` を付け、既存ファイルがそのまま読めること。
11. パッケージ名 `lol-overlay` と lib 名 `lol_overlay_lib`(src-tauri)は変えない(Windows ビルドの制約)。

---

## 4. Phase 0 — workspace 化(ファイル移動なし)

### 作業

1. リポジトリルートに `Cargo.toml` を新規作成:

```toml
[workspace]
resolver = "2"
members = ["src-tauri", "crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2021"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
async-trait = "0.1"
tokio = { version = "1", features = ["time", "sync"] }
# native-tls on purpose — see invariant 4.
reqwest = { version = "0.12", default-features = false, features = ["json", "native-tls"] }
irelia = { version = "0.11", features = ["ws"] }
# internal crates (uncomment as phases land)
# overlay-types = { path = "crates/types" }
# overlay-ddragon = { path = "crates/ddragon" }
# overlay-lcu = { path = "crates/lcu" }
# overlay-live-client = { path = "crates/live-client" }
# overlay-provider = { path = "crates/provider" }
# overlay-provider-deeplol = { path = "crates/provider-deeplol" }
# overlay-provider-ugg = { path = "crates/provider-ugg" }
```

2. `src-tauri/Cargo.toml` の依存を `{ workspace = true }` 参照に書き換える(バージョンの一元管理)。`[lib]` セクション・tauri 系依存はそのまま。
3. `.gitignore` に `/target` を追加(ビルド成果物がルート `target/` に移るため)。
4. `crates/` ディレクトリを作成(空で良い。`crates/*` glob は空でもエラーにならないが、気になるなら Phase 1 と同時で可)。

### 検証

```bash
cargo check --workspace
cargo test --workspace --lib
pnpm build          # tsc + vite(フロント不変の確認)
```

---

## 5. Phase 1 — `overlay-types`(純粋データ型クレート)

### 方針

uggo の `ugg-types` に相当。**プロバイダ・LCU・Live Client・フロントイベントで共有される serialize 可能な型**をここに集める。依存は `serde` のみ。

### `crates/types/` 構成と移動元

| 新ファイル | 移動する型 | 移動元 |
|---|---|---|
| `src/lib.rs` | モジュール公開のみ | — |
| `src/snapshot.rs` | `GameSnapshot`, `EnemyChampion` | `src-tauri/src/live_client.rs` |
| `src/recommendation.rs` | `ThreatProfile`, `ItemRecommendation`, `SkillOrder`, `RuneRecommendation`, `TierEntry`, `CounterEntry`, `RuneBuild` | `src-tauri/src/provider/mod.rs` |
| `src/lcu.rs` | `Phase`, `MyPick`, `RunePagePayload`, `SummonerInfo`, `RecentGame` | `src-tauri/src/lcu.rs` |
| `src/champ_select.rs` | `ChampSelectEvent` | `src-tauri/src/events.rs` |

注意:

- serde 属性(`rename_all = "camelCase"` 等)・doc コメント・フィールド順を**そのまま**持っていく。
- `Phase::label()` などの impl も型と一緒に移動。
- 移動元では `pub use overlay_types::...;` で再エクスポートして既存の `crate::events::ChampSelectEvent` 等のパスを生かす(後続フェーズで参照側を整理するまでの橋)。
- `Raw*` 系 DTO(`RawPlayer`, `AllGameData` 等)は **live_client の内部表現なので移動しない**。

### 検証

```bash
cargo check --workspace && cargo test --workspace --lib && pnpm build
```

---

## 6. Phase 2 — `overlay-ddragon`(Data Dragon 共有クライアント)

### 方針

deeplol.rs に埋まっている Data Dragon 処理を独立クレートに抽出する。**deeplol と ugg の両プロバイダが同一インスタンス(`Arc<DdragonClient>`)を共有**し、静的マップの二重フェッチを防ぐ。

### 抽出対象(`src-tauri/src/provider/deeplol.rs` 内)

- `fetch_ddragon_version()`(L474–486 付近)
- `fetch_champion_map()`(L492–522 付近)— `name_to_id` / `id_to_name` / `id_to_image` に加え、**数値 `key`(`id_to_key: HashMap<i64, String>`…u.gg の URL が要求)も保持するよう拡張**(`DDChampion` は `key` フィールドを既に持つはず。無ければ追加)
- `fetch_item_map()`(L524–540 付近)
- `normalize()`(名前正規化、L960–965 付近)と関連 DTO(`DDChampionFile`, `DDChampion`, `DDItemFile`, `DDItem`)
- 定数 `DDRAGON`

### 公開 API(`crates/ddragon/src/lib.rs`)

```rust
pub struct DdragonClient { /* reqwest::Client + RwLock<Option<StaticData>> */ }

pub struct ChampionMaps {
    pub name_to_id: HashMap<String, i64>,   // normalized name -> id
    pub id_to_name: HashMap<i64, String>,   // display name ("Cho'Gath")
    pub id_to_image: HashMap<i64, String>,  // ddragon image id ("Chogath")
    pub id_to_key: HashMap<i64, String>,    // numeric key as string ("31")
}

impl DdragonClient {
    pub fn new() -> Self;
    pub async fn version(&self) -> Result<String, DdragonError>;
    pub async fn champions(&self) -> Result<Arc<ChampionMaps>, DdragonError>; // cached for process lifetime
    pub async fn items(&self) -> Result<Arc<HashMap<i64, String>>, DdragonError>; // id -> display name
}

pub fn normalize(name: &str) -> String;

#[derive(thiserror::Error, Debug)]
pub enum DdragonError { Http(#[from] reqwest::Error), Other(String) }
```

- 8 秒タイムアウトなど既存の reqwest クライアント設定を踏襲。
- このフェーズでは deeplol.rs を**まだ書き換えない**(Phase 5 でまとめて差し替え)。クレートを作り、単体でコンパイル+既存ロジックのコピーであることを確認するだけ。重複は Phase 5 で解消する。

### 検証

```bash
cargo check --workspace && cargo test -p overlay-ddragon
```

---

## 7. Phase 3 — `overlay-lcu`(irelia ラッパー)

### 方針

uggo の `lol-client` クレートに相当。**`irelia` への依存をこのクレートに完全に閉じ込める。** 公開シグネチャに `irelia::` 型を一切出さない(grep で `irelia` が `crates/lcu/` の外に現れないこと)。

### `crates/lcu/` 構成

| 新ファイル | 内容 | 移動元 |
|---|---|---|
| `src/lib.rs` | 公開 API(下記)、re-export | — |
| `src/rest.rs` | `fetch_summoner` / `fetch_recent_matches` / `fetch_platform_id` / `fetch_phase` / `fetch_session` / `apply_runes` / `apply_spells` と内部ヘルパ(`platform_id_from_region`, `deletable_page_id`, `is_remake`, `parse_recent_matches`) | `src-tauri/src/lcu.rs` |
| `src/parse.rs` | `parse_my_pick` / `parse_champ_select`(純パース関数。ネットワーク非依存) | 同上 |
| `src/ws.rs` | `SessionForwarder` + `subscribe_champ_select` | 同上 |
| `src/error.rs` | `LcuError`(thiserror) | 新規 |

### 公開 API

```rust
// crates/lcu/src/lib.rs
pub use overlay_types::{ChampSelectEvent, MyPick, Phase, RecentGame, RunePagePayload, SummonerInfo};

#[derive(thiserror::Error, Debug)]
pub enum LcuError {
    #[error("LCU unavailable: {0}")]
    Unavailable(String),   // lockfile not found / client closed
    #[error("LCU error: {0}")]
    Other(String),
}

pub async fn fetch_summoner() -> Result<SummonerInfo, LcuError>;
pub async fn fetch_recent_matches(count: usize) -> Result<Vec<RecentGame>, LcuError>;
pub async fn fetch_platform_id() -> Result<String, LcuError>;
pub async fn fetch_phase() -> Result<Phase, LcuError>;
pub async fn fetch_session() -> Result<Option<serde_json::Value>, LcuError>;
pub async fn apply_runes(page: &RunePagePayload) -> Result<(), LcuError>;
pub async fn apply_spells(spell1: i64, spell2: i64) -> Result<(), LcuError>;

pub fn parse_my_pick(session: &serde_json::Value) -> Option<MyPick>;
pub fn parse_champ_select(session: &serde_json::Value) -> Option<ChampSelectEvent>;

/// Subscribes to champ-select session events and forwards payloads onto `tx`.
/// The underlying socket handle is intentionally leaked so the subscription
/// lives for the lifetime of the process (mirrors previous mem::forget in lib.rs).
pub fn subscribe_champ_select(tx: tokio::sync::mpsc::UnboundedSender<serde_json::Value>) -> Result<(), LcuError>;
```

注意:

- **`mem::forget` は wrapper 内部に移す**。呼び出し側(`src-tauri/src/lib.rs`)からハンドル管理の知識を消す。
- irelia のエラーは `LcuError` に変換(現行の `format!("{e:?}")` 踏襲で良い)。204 応答が irelia の RmpDecode EOF として表面化する件の取り扱い(握りつぶし)も現行どおり移植する。
- `lcu.rs` 内の単体テスト(セッション JSON パース 9 本)は `parse.rs` / `rest.rs` の `#[cfg(test)]` に**そのまま**移動。
- アプリの `error.rs` に `From<LcuError> for Error` を追加し、既存呼び出し側の `?` を生かす。

### 検証

```bash
cargo test -p overlay-lcu
grep -rn "irelia" src-tauri/src crates --include='*.rs' | grep -v crates/lcu   # 出力ゼロであること
cargo check --workspace && cargo test --workspace --lib
```

---

## 8. Phase 4 — `overlay-live-client`

### 作業

- `src-tauri/src/live_client.rs` を `crates/live-client/src/lib.rs` へ移動。
- `GameSnapshot` / `EnemyChampion` は Phase 1 で `overlay-types` へ移動済みなので import を差し替え。`Raw*` DTO・`LiveClient` 構造体・`snapshot()` はこのクレートに残す。
- reqwest クライアント設定(native-tls + invalid certs 許容 + タイムアウト)を変えない。

### 検証

```bash
cargo check --workspace && cargo test --workspace --lib
```

---

## 9. Phase 5 — `overlay-provider`(trait コア)+ `overlay-provider-deeplol`

2 つのチェックポイントに分けて進める。

### 9a. `overlay-provider`(trait / error / threat / hardcoded)

| 新ファイル | 内容 | 移動元 |
|---|---|---|
| `src/lib.rs` | re-export | — |
| `src/error.rs` | `ProviderError`(新規、下記) | `src-tauri/src/error.rs` の意味論を分離 |
| `src/trait_def.rs` | `BuildProvider` trait(シグネチャは現行どおり、`Result` の型のみ差し替え) | `src-tauri/src/provider/mod.rs` |
| `src/threat.rs` | `classify_threats` | 同上 |
| `src/hardcoded.rs` | `HardcodedProvider`, `champion_damage_type` | `src-tauri/src/provider/hardcoded.rs` |
| `src/proxy.rs` | `ProviderProxy`, `ProviderKind`(Phase 6 で配線。型はここで定義して良い) | 新規 |

```rust
// crates/provider/src/error.rs
#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("not enough data")]
    NotEnoughData,
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}
pub type Result<T> = std::result::Result<T, ProviderError>;
```

- trait のデフォルト実装の `Err(crate::error::Error::NotEnoughData)` は `Err(ProviderError::NotEnoughData)` に置換。それ以外のシグネチャ・doc コメントは不変。
- **アプリ側 `src-tauri/src/error.rs`**: 既存 `Error` はそのまま残し、`From<ProviderError> for Error` を追加。`ProviderError::NotEnoughData → Error::NotEnoughData` のマッピング必須(不変条件 3 の `"not-enough-data"` literal を守るため)。

### 9b. `overlay-provider-deeplol`

- `src-tauri/src/provider/deeplol.rs`(1772 行)を `crates/provider-deeplol/src/lib.rs` へ**原則そのまま**移動。許可される変更:
  1. import パスの差し替え(`overlay_types::…`, `overlay_provider::…`)。
  2. エラー型を `ProviderError` に置換(variant 対応は機械的)。
  3. **Data Dragon 部分の差し替え**: `fetch_ddragon_version` / `fetch_champion_map` / `fetch_item_map` / `normalize` / DD DTO を削除し、コンストラクタ注入の `Arc<DdragonClient>` を使う形へ。`Cache` 構造体の `name_to_id` 等のフィールドは `DdragonClient` 参照に置き換える(`ensure_static` がその初期化点)。
  4. 任意: DTO 群(L967–1262 付近)を `src/dto.rs` へ、整形ヘルパ(`tier_rows`, `counter_entries`, `aggregate_otp`, `most_common_spell_pair`, `mode`, `pick_lane` 等)を `src/shape.rs` へ分割。**分割は機械的な移動のみ**。
- コンストラクタは `DeepLolProvider::new(ddragon: Arc<DdragonClient>)` に変更。
- 単体テスト 14 本・ignored ネットワークテスト 7 本を 1:1 で移動(ignored は ignored のまま)。
- `src-tauri/src/provider/` ディレクトリは削除し、`lib.rs::run` の生成箇所を新クレート参照に差し替え。

### 検証

```bash
cargo test --workspace --lib
cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture   # ネットワーク必須。全 pass を確認
pnpm build
pnpm tauri dev   # 起動し、Ctrl+Shift+D のモックで champ-select / in-game パネルが出ること(Mac で可)
```

---

## 10. Phase 6 — `ProviderProxy` 配線とデータソース切替

### 10.1 `ProviderKind` と `ProviderProxy`(`crates/provider/src/proxy.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Deeplol,
    Ugg,
}

impl ProviderKind {
    pub fn parse(s: &str) -> Option<Self>;       // "deeplol" | "ugg"
    pub fn as_str(&self) -> &'static str;
}

/// Routes every BuildProvider call to the currently active backend.
/// Switching is O(1); each backend keeps its own caches, so flipping
/// back and forth costs nothing after warm-up.
pub struct ProviderProxy {
    providers: HashMap<ProviderKind, Arc<dyn BuildProvider>>,
    active: std::sync::RwLock<ProviderKind>,
}

impl ProviderProxy {
    pub fn new(initial: ProviderKind) -> Self;
    pub fn register(&mut self, kind: ProviderKind, provider: Arc<dyn BuildProvider>);
    pub fn set_active(&self, kind: ProviderKind) -> Result<()>;  // Err(Other) if not registered
    pub fn active(&self) -> ProviderKind;
    pub fn available(&self) -> Vec<ProviderKind>;
    fn current(&self) -> Arc<dyn BuildProvider>;  // clone the Arc under the lock, then drop the guard
}

#[async_trait]
impl BuildProvider for ProviderProxy {
    // set_platform_id: forward to ALL registered providers (region must be
    //   known even for the inactive backend when the user switches later).
    // every async method: `self.current().method(args).await`
    //   — IMPORTANT: never hold the RwLock guard across an await point.
}
```

### 10.2 アプリ側の配線

- `engine.rs`: `Engine.provider` の型を `Arc<dyn BuildProvider>` → `Arc<ProviderProxy>` に変更(`BuildProvider` を impl しているので呼び出し側コードは不変)。
- `Settings` に `#[serde(default)] pub data_source: ProviderKind` を追加(不変条件 10)。
- `lib.rs::run`:

```rust
let ddragon = Arc::new(DdragonClient::new());
let mut proxy = ProviderProxy::new(settings.data_source);
proxy.register(ProviderKind::Deeplol, Arc::new(DeepLolProvider::new(ddragon.clone())));
proxy.register(ProviderKind::Ugg, Arc::new(UggProvider::new(ddragon.clone())));  // Phase 7 後に有効化
let provider = Arc::new(proxy);
```

- `commands.rs` に追加:
  - `get_data_source(engine) -> String`
  - `list_data_sources(engine) -> Vec<String>`
  - `set_data_source(app, engine, kind: String) -> Result<()>` — parse → `proxy.set_active` → `Settings.data_source` 更新+永続化 → `app.emit("data-source", kind)`。
- `generate_handler![]` に 3 コマンドを登録。

### 10.3 フロントエンド(最小変更)

- `src/types.ts`: `Settings` interface に `dataSource: string` を追加。
- `src/state/settings.ts`: `dataSource` シグナルと `set_data_source` invoke を追加。
- `src/components/SettingsPanel.tsx`: データソース選択(`list_data_sources` の結果でセレクト/トグル描画)。
- `src/state/caches.ts`: `data-source` イベントを listen し、tier list / counters / rune build のメモリキャッシュを全クリア(切替後の再フェッチは既存の取得導線に任せる)。

### 検証

```bash
cargo test --workspace --lib && pnpm build
pnpm tauri dev   # 設定パネルでデータソース表示・切替ができ、エラーが出ないこと(ugg 未実装の間は deeplol のみ表示)
```

---

## 11. Phase 7 — `overlay-provider-ugg`(新規実装)

### 11.1 データ源(参考実装からの裏取り済み)

移植元: `reference-repo.local/crates/ugg-api/src/lib.rs` と `reference-repo.local/crates/ugg-types/src/`。
参考実装は同期(`ureq`)+ `RefCell<LruCache>` なので、**reqwest(async)+ `RwLock<HashMap>`(プロセス生涯キャッシュ)に書き換えて移植**する。`simd-json` / `lru` / `levenshtein` は持ち込まない(serde_json と完全一致検索で足りる)。

URL(実コードで確認済みの形):

```
API バージョン表:
  https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json
  → { "15_12": { "overview": "1.5.0", "matchups": "1.5.0" }, ... }

ビルド概要:
  https://stats2.u.gg/lol/1.5/{build}/{patch}/{mode}/{champion_key}/{api_version}.json
  例: https://stats2.u.gg/lol/1.5/overview/15_12/ranked_solo_5x5/103/1.5.0.json
  - build:        "overview"(v1 はこれのみ)
  - patch:        ddragon version "15.12.1" → "15_12"(major_minor)
  - mode:         "ranked_solo_5x5" | "normal_aram"
  - champion_key: Data Dragon の数値 key 文字列(Ahri = "103")→ overlay-ddragon の id_to_key
  - api_version:  バージョン表から。無ければ "1.5.0" にフォールバック

マッチアップ:
  https://stats2.u.gg/lol/1.5/matchups/{patch}/{mode}/{champion_key}/{api_version}.json
```

レスポンスは `HashMap<Region, HashMap<Rank, HashMap<Role, Wrapped…>>>` 形(キーは数値文字列の enum)。**JSON が配列ベース**なので、`ugg-types` のカスタム `Deserialize` visitor(`default_overview.rs` / `overview.rs` / `matchups.rs` / `mappings.rs` / `arena_overview.rs`)を**そのまま移植**する(自作しない。ここが一番壊れやすい)。

### 11.2 クレート構成

| ファイル | 内容 |
|---|---|
| `src/lib.rs` | `UggProvider`(`impl BuildProvider`) |
| `src/api.rs` | HTTP + キャッシュ層(`get_overview` / `get_matchups` / `get_api_versions`) |
| `src/types/…` | `ugg-types` から移植: `mappings.rs`, `overview.rs`, `default_overview.rs`, `arena_overview.rs`, `matchups.rs` |

`UggProvider::new(ddragon: Arc<DdragonClient>)`。内部状態:

- `platform_id: RwLock<String>`(`set_platform_id` で更新)
- `RwLock<Option<UggStatic>>`: patch(`"15_12"`)+ api-versions 表(初回 `ensure_static` で確定)
- `RwLock<HashMap<(i64 /*champ*/, Mode), ChampOverview>>` と同様の matchups キャッシュ(プロセス生涯。不変条件 8)

ランク/リージョンの選択は参考実装と同じ規則: 自リージョン(`platform_id` → `Region`、未知は `Region::World`)→ 無ければ `Region::World`、ランクは `Rank::preferred_order()` の先頭ヒット。ロールは指定ロール → 無ければデータ最多ロール(参考実装 `get_stats` の挙動を踏襲)。

### 11.3 `BuildProvider` メソッド対応表

| trait メソッド | u.gg データ | 実装メモ |
|---|---|---|
| `set_platform_id` | — | `"JP1"` → `Region::JP1` 等のマップを保持。未知は World |
| `items(snapshot)` | overview の `starting_items` / `core_items` / `item_4_options`〜`item_6_options` | **出力契約(`ItemRecommendation` の score 順・reason 文体)は deeplol の `items()`(deeplol.rs L561–599)を読み、同じ形に整形**。mode は `snapshot.game_mode == "ARAM"` → `normal_aram`、それ以外 `ranked_solo_5x5`。Arena は `NotEnoughData` |
| `skill_order(snapshot)` | overview の `abilities`(`ability_order: Vec<char>`, `ability_max_order: String`) | `Q/W/E/R` → `1/2/3/4` に変換して `level_order: Vec<i64>` へ。`max_order` 文字列は deeplol の出力形式(deeplol.rs L601–635 を確認)に合わせる |
| `runes(champ, role)` | overview の `runes`(primary_style_id / secondary_style_id / rune_ids) | ページ名は deeplol と同じ命名規則を踏襲(`rune_build_from_entry` の名前生成を確認して合わせる) |
| `tier_list(role)` | **非対応** | `Err(ProviderError::NotEnoughData)`(デフォルト実装のまま)。理由: stats2 にはサイト全体の集計 tier list JSON が無く、全チャンピオン分の per-champion fetch は重すぎる。UI は `SectionError` 表示で劣化許容。将来 u.gg 内部 GraphQL 対応の余地をコメントで残す |
| `counters(champ, role)` | matchups の `worst_matchups`(自分が負ける相手 = カウンター) | `CounterEntry.win_rate` は**相手視点**(deeplol の `counter_entries` と同じ向き): `1.0 - wins/matches`。`games = matches`。30 games 未満は除外、勝率降順、最大 8 件(deeplol の `MIN_MATCHUP_GAMES` / cap と揃える) |
| `rune_build(champ, role, enemy)` | overview の `runes` + `shards.shard_ids` + `summoner_spells` + wins/matches | `enemy_champion_id` が `Some` の場合は `Err(NotEnoughData)`(u.gg のこの API にマッチアップ別ルーンは無い。フロントは matchup タブのフォールバック表示を既に持つ)。`matchup: false` 固定 |
| `champion_names(id)` | — | `overlay-ddragon` の `id_to_name` / `id_to_image` から(deeplol と同一ソースなので表示も同一) |

### 11.4 テスト

- 単体: `Q/W/E/R` 変換、platform_id → Region マップ、patch 文字列変換(`"15.12.1"` → `"15_12"`)、counters の視点反転と 30 games フィルタ、visitor 移植分のフィクスチャパース(参考実装のテストがあれば流用)。
- `#[ignore]` ネットワークテスト: Ahri(103)の overview 取得 → items / skill_order / rune_build が `Ok` で妥当な形(perk id が 5000 番台、item id が正の整数、win_rate が 0–1)であることを `--nocapture` で目視できる形にする。

### 11.5 配線

- Phase 6 でコメントアウトしていた `proxy.register(ProviderKind::Ugg, …)` を有効化。
- `list_data_sources` に `ugg` が現れ、設定 UI から切替可能になることを確認。

### 検証

```bash
cargo test -p overlay-provider-ugg --lib
cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture   # 実 API 疎通
cargo test --workspace --lib && pnpm build
pnpm tauri dev   # モック(Ctrl+Shift+D)+ データソースを ugg に切替 → in-game パネルにアイテム/スキルオーダーが出る
```

---

## 12. Phase 8 — 仕上げ

1. **橋渡し re-export の整理**: Phase 1 で残した `pub use` 橋(`src-tauri` 内の旧パス)を削除し、参照側を正規パス(`overlay_types::…` 等)に統一。
2. **workspace lints**(任意だが推奨。参考実装と同様):

```toml
# ルート Cargo.toml
[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
missing_errors_doc = "allow"
module_name_repetitions = "allow"
```

   各クレートに `[lints] workspace = true`。**警告の機械的修正で挙動を変えないこと**。修正が大きくなる場合は `#[allow]` で逃がす。
3. **CLAUDE.md 更新**: アーキテクチャ章をクレート構成に合わせて書き換え。テストコマンドを更新:
   - `cargo test --workspace --lib`(ルートから)
   - `cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture`
   - `cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture`
   - 「Data provider abstraction」節に `ProviderProxy` / `ProviderKind` / データソース切替コマンドを追記。
4. **最終検証**(下記マトリクス全項目)。

---

## 13. 検証マトリクス(最終)

| 項目 | コマンド / 手順 | 期待 |
|---|---|---|
| 全クレートビルド | `cargo check --workspace` | 警告以外なし |
| 全単体テスト | `cargo test --workspace --lib` | 既存 26 本+新規が全 pass |
| deeplol 実 API | `cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture` | 7 本 pass(要ネットワーク) |
| ugg 実 API | `cargo test -p overlay-provider-ugg --lib -- --ignored --nocapture` | pass(要ネットワーク) |
| irelia 封じ込め | `grep -rn "irelia" src-tauri crates --include='*.rs' \| grep -v crates/lcu` | 出力 0 行 |
| フロント型チェック | `pnpm build` | tsc エラーなし |
| モック動作 | `pnpm tauri dev` → Ctrl+Shift+D を 2 回 | champ-select パネル → in-game パネルがデータ付きで表示 |
| 切替動作 | 設定パネルで deeplol ⇄ ugg 切替 | エラーなし、パネル再描画、`settings.json` に `dataSource` 永続化 |
| 旧設定互換 | 既存 `settings.json`(`dataSource` 無し)で起動 | deeplol で起動し例外なし |
| Windows 実機 | LoL クライアント起動 → champ-select → 試合 | ルーン自動インポート / HEXGATE / in-game 推薦が従来どおり(ユーザーが実施) |

---

## 14. 参考: 参照すべきファイル早見表

| 知りたいこと | 場所 |
|---|---|
| 目標とする workspace の手本 | `reference-repo.local/Cargo.toml`, `reference-repo.local/crates/*` |
| u.gg URL 組み立て・rank/region フォールバック | `reference-repo.local/crates/ugg-api/src/lib.rs`(L161–270 付近) |
| u.gg レスポンスのカスタム visitor | `reference-repo.local/crates/ugg-types/src/{overview,default_overview,matchups,mappings}.rs` |
| LCU ラッパーの手本(ureq 版) | `reference-repo.local/crates/lol-client/src/lib.rs` |
| 現行 trait と共有型 | `src-tauri/src/provider/mod.rs` |
| 現行 deeplol の整形契約(items/skill/counters/runes) | `src-tauri/src/provider/deeplol.rs` |
| イベント payload の TS 側ミラー | `src/types.ts`, `src/state/backend.ts` |
| 既知の罠まとめ | `CLAUDE.md` の「Non-obvious gotchas」 |
