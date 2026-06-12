# 移行計画書: Vanilla TS → SolidJS + Tailwind CSS v4

実装担当モデルへ: このドキュメントは lol-overlay のフロントエンドを SolidJS + Tailwind CSS v4 へ移行するための完全な仕様である。**着手前に必ず「§2 不変条件」と「§8 Solid 実装ガイド」を読むこと。** フェーズごとにチェックポイントがあり、各チェックポイントで `pnpm build` が通り、UI が壊れていないことを確認してから次へ進む。

---

## §1 ゴールと非ゴール

### ゴール

- `src/main.ts`(604行) と `src/champselect.ts`(838行) の手書き DOM 操作 + 自前レンダリングシステム(`scheduleRender`/`renderAll`/render-key guards)を SolidJS のシグナルベース実装に置き換える。
- `src/styles.css`(1,353行) を Tailwind v4 のユーティリティクラス + `@theme` トークンに置き換える。複雑な装飾(後述)は少量のカスタム CSS として残してよい。
- 見た目・挙動は**現状と完全一致**が合格ライン(ピクセル単位の厳密一致までは求めないが、レイアウト・色・フォント・アニメーション・操作感はすべて維持)。

### 非ゴール(やらないこと)

- **Rust 側(`src-tauri/`)は1行も変更しない。** イベント名・ペイロード・コマンド名はすべて現状のまま。
- `src/types.ts` のインターフェイス定義の変更(Rust 側 serde と 1:1 対応している)。
- `src/assets.ts` のロジック変更(§7 で軽微な追加のみ)。
- 機能追加・デザイン変更・リファクタリング名目の挙動変更。
- `tauri.conf.json` / `index.html` の Google Fonts リンクの変更。

---

## §2 不変条件 (MUST — 違反すると実機で壊れる)

このアプリは透明・常時最前面・クリックスルーのオーバーレイウィンドウで、ゲームの上に重なって動く。以下は仕組み上の必須要件であり、移行後も**完全に同じ動作**でなければならない。

1. **`data-hit` 属性によるヒット領域。** OS のクリックスルーはウィンドウ単位でしか切れないため、フロントが可視の `[data-hit]` 要素の rect を `invoke("set_hit_regions")` で報告し、バックエンドがカーソル位置と照合してクリックスルーを切り替えている。`data-hit` を付ける要素は現状と同一にする:
   - 設定パネルのルート (`#settings` 相当)
   - in-game パネルのヘッダー (`.ig-head` 相当)
   - HEXGATE パネルのルート (`#hexgate` 相当、パネル全体)
2. **ヒット領域の報告タイミング。** 250ms 間隔のポーリング + ホットパス(ドラッグ終了、折りたたみ切替、width の transitionend)での直接呼び出し。`reportHitRegions` の重複抑止(rect 配列の JSON 文字列比較)も維持する。非表示要素は rect が 0×0 になり報告から除外される — `<Show>` でアンマウントされた要素は querySelectorAll に出てこないので同じ結果になる(問題なし)。
3. **`pointer-events` の構造。** `html, body { pointer-events: none }` が基本で、`.recs` / `.settings` / `.hexgate` のパネルルートだけ `pointer-events: auto`。これを崩すとゲームへのクリックが吸われる。
4. **パネルドラッグの実装。** in-game パネルは pointer capture による自前ドラッグ(`style.left/top` 直接書き換え)+ ドラッグ中は `invoke("set_drag_active", { active: true })` でウィンドウを操作可能に固定し、終了時に `false` + 位置保存(`set_ingame_panel_position`) + `reportHitRegions()`。HEXGATE ヘッダーは `appWindow.startDragging()` による**ウィンドウごと**のドラッグで、`onMoved` を 250ms デバウンスして `set_champselect_window_position` に保存(champselect モード時のみ)。`shouldStartDrag` のインタラクティブ要素除外(`button, input, label, select, textarea, a`)も維持。
5. **折りたたみアニメーション。** in-game パネルの折りたたみは `.ig-collapse-wrap` の `grid-template-rows: 1fr ⇔ 0fr` transition で実現している(中身を保持したまま高さが動く)。**ここは `<Show>` でアンマウントしてはいけない** — クラス切替(`collapsed`)のまま移植する。パネルルートの `width` の `transitionend` で `clampPanelToViewport` + 位置保存 + ヒット領域報告。
6. **イベント・コマンド契約。** Tauri イベント(`phase`, `recommendations`, `rune-imported`, `summoner`, `match-history`, `lp-change`, `log`, `window-mode`, `interactive`, `champ-select`)とコマンド(`get_settings`, `get_ui_layout`, `set_auto_import`, `set_import_spells`, `set_spells_flipped`, `set_pinned`, `set_ingame_collapsed`, `set_ingame_panel_position`, `set_champselect_window_position`, `set_hit_regions`, `set_drag_active`, `get_tier_list`, `get_counters`, `get_rune_build`, `import_build`)の名前・ペイロードは一切変更しない。ペイロードは camelCase(`types.ts` が正)。
7. **DPI/ズーム補正の禁止。** UI は CSS 固定 px 設計。`html { font-size }` の変更、rem ベースのスケーリング導入、`set_zoom` 等による補正を**一切しない**。Tailwind の spacing は rem 基準だがルート 16px 固定なのでそのまま使ってよい(16px = `4` = 1rem)。
8. **`focusable: false` ウィンドウ。** フォーカス依存の UI(autofocus、:focus 前提の操作)を入れない。現状どおりクリック/ホバーのみで完結させる。
9. **失敗を握りつぶす invoke。** バックエンド呼び出しは現状 `.catch(() => {})` で fire-and-forget のものが多い。この方針を変えない(クライアント未起動時に console がエラーで溢れるのを防いでいる)。

---

## §3 現状アーキテクチャ(移行に必要な範囲)

着手前に以下を読むこと: `src/main.ts`, `src/champselect.ts`, `src/types.ts`, `src/assets.ts`, `src/styles.css`, `index.html`, `CLAUDE.md`。

- `index.html` に全パネルの静的マークアップがあり、TS が `getElementById` で掴んで `replaceChildren` で書き換える方式。
- `main.ts`: in-game 系パネル(ステータスチップ+プロフィール+戦績、LPバナー、ルーンバナー、設定、in-game ビルドパネル)、ドラッグ、ヒット領域、レイアウト永続化。
- `champselect.ts`: HEXGATE パネル。モジュールレベル `let` の状態 + `scheduleRender()`(microtask で renderAll を coalesce) + セクションごとの render-key guard(入力が変わったときだけ DOM 再構築)という**自前リアクティブシステム**。これが Solid のシグナルでそのまま置き換わる。
- `makeCache`(champselect.ts): invoke 結果の `loading | ok | err` 3状態キャッシュ。解決時に `scheduleRender()`。→ Solid ではシグナル入りキャッシュに置き換える(§8)。
- `assets.ts`: Data Dragon の名前/アイコン解決。モジュールレベルの Map で、`initAssets()` 完了後に有効になる。完了前に呼ぶと空文字が返る。現状は `assetsVersion` カウンタで再レンダーを強制している。

---

## §4 採用スタックと追加依存

```bash
pnpm add solid-js
pnpm add -D vite-plugin-solid tailwindcss @tailwindcss/vite
```

- **SolidJS** (1.9 系最新): シグナルベース・VDOM なし。ゲームと同居するため実行時 CPU 最小を優先。
- **Tailwind CSS v4** (`@tailwindcss/vite` プラグイン使用、PostCSS 設定不要、`tailwind.config.js` も不要 — CSS 内 `@theme` で設定する)。
- バージョンは pnpm が解決する最新でよい。手動ピン留め不要。

### vite.config.ts (最終形)

```ts
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [solid(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
}));
```

### tsconfig.json (compilerOptions に追加)

```jsonc
"jsx": "preserve",
"jsxImportSource": "solid-js",
```

注意: `strict` + `noUnusedLocals` + `noUnusedParameters` が有効。`pnpm build` は `tsc && vite build` なので未使用変数1つでビルドが落ちる。

---

## §5 新ディレクトリ構成

```
index.html              シェルのみ: フォントリンク + <div id="root"> + /src/index.tsx
src/
  index.tsx             エントリ: render(<App/>), ヒット領域インターバル開始, initAssets()
  App.tsx               全パネルを並べるだけのルート
  app.css               @import "tailwindcss" + @theme + base層 + 残存カスタムCSS
  types.ts              [変更なし]
  assets.ts             [ほぼ変更なし] assetsReady シグナルを追加 (§7)
  state/
    backend.ts          Tauri listen → シグナル群 / invoke ラッパ
    settings.ts         Settings ストア (autoImport, importSpells, spellsFlipped, pinned)
    caches.ts           makeCache のシグナル版 + tierCache/counterCache/buildCache
    layout.ts           UiLayout の読込/保存, in-game パネル位置・折りたたみ状態
  lib/
    hitRegions.ts       reportHitRegions + 250ms インターバル (main.ts から移植)
    drag.ts             パネルドラッグ / ウィンドウドラッグ (main.ts から移植)
  components/
    Icon.tsx            setIcon をリアクティブ化した <img> ラッパ (§8.4)
    StatusChip.tsx      接続状態 + プロフィール + 直近戦績ストリップ
    LpBanner.tsx        試合後 LP 変動バナー (12s 自動消滅)
    RuneBanner.tsx      ルーンインポート完了バナー (6s 自動消滅)
    SettingsPanel.tsx   設定パネル (data-hit)
    ingame/
      InGamePanel.tsx   in-game ビルドパネル本体 (ドラッグ/折りたたみ/data-hit ヘッダー)
      SkillOrder.tsx    スキルオーダー行 (getAbility 非同期解決)
    hexgate/
      HexgatePanel.tsx  表示判定 + ヘッダー (ウィンドウドラッグ, phase チップ, pin, gear)
      RoleChips.tsx     ロール選択チップ (LCU がロールをくれない時のみ表示)
      TierLists.tsx     STRONG PICKS / BAN TARGETS (スケルトン/エラー/リトライ込み)
      EnemyRow.tsx      敵5スロット
      Counters.tsx      カウンターストリップ
      Matchup.tsx       マッチアップ表示行
      Tabs.tsx          BEST BUILD / VS タブ + VS ドロップダウン
      BuildArea.tsx     ルーンページ / スケルトン / empty / not-enough-data / エラー
      StatsRow.tsx      WR・games・スペルアイコン・FLIP・Spells トグル
      ImportButton.tsx  インポートボタン (idle/importing/imported/failed)
```

削除するファイル: `src/main.ts`, `src/champselect.ts`, `src/styles.css`(フェーズ3完了時)。

---

## §6 フェーズ計画

各フェーズの最後に **チェックポイント** がある。通らなければ次へ進まない。

### フェーズ 0: ツールチェーン導入(UI 無変更)

1. §4 のとおり依存追加、`vite.config.ts` と `tsconfig.json` を更新。
2. この時点ではエントリは `main.ts` のまま。`app.css` もまだ読み込まない。

**チェックポイント:** `pnpm build` が通る。`pnpm tauri dev` で従来 UI が無変化で動く。

### フェーズ 1: 状態レイヤー構築(UI 無変更)

`state/`, `lib/`, `components/Icon.tsx` を新規作成する。既存コードからの移植元と仕様は §7・§8 を参照。既存の `main.ts`/`champselect.ts` はまだ触らない(新モジュールはどこからも import されていなくてよい — tsc はモジュール単位の未使用 export を咎めない)。

**チェックポイント:** `pnpm build` が通る。

### フェーズ 2: Solid 移植(既存 CSS クラスをそのまま使う)

**このフェーズでは Tailwind ユーティリティをまだ使わない。** 既存の `styles.css` のクラス名(`panel`, `recs`, `ig-head`, `hx-row`, …)をそのまま JSX の `class` に書き、視覚的равность(=見た目の同一性)を CSS 側で担保したまま挙動だけを Solid に移す。マークアップは `index.html` と旧 TS の DOM 構築コードから忠実に転写する。

1. `index.html` を縮小: `<body>` 内を `<div id="root"></div>` のみに。`<script src="/src/index.tsx">` に差し替え。`<link rel="stylesheet" href="/src/styles.css" />` と Google Fonts リンクは残す。
2. `src/index.tsx` 作成: `render(() => <App />, document.getElementById("root")!)`、`lib/hitRegions.ts` のインターバル開始、`initAssets()` 起動、`window resize` ハンドラ。
3. 全コンポーネントを実装(§7 の状態マッピング表に従う)。
4. `src/main.ts`, `src/champselect.ts` を削除。

**チェックポイント:** `pnpm build` が通る。`pnpm tauri dev` + `Ctrl+Shift+D`(モックモード)で champ-select シーン → in-game シーンが順に流れるので、§10 の動作チェックリストを全項目確認。見た目は移行前と同一であること。

### フェーズ 3: Tailwind 化

1. `src/app.css` を作成し `index.tsx` から import(`index.html` の styles.css リンクはまだ残す):

```css
@import "tailwindcss";

@theme {
  --color-hx-bg: #0b0a08;
  --color-hx-bg-raised: #15120d;
  --color-hx-border: #3a3022;
  --color-hx-gold: #c8aa6e;
  --color-hx-gold-dim: #7a6a4d;
  --color-hx-text: #f0e6d2;
  --color-hx-muted: #847b6a;
  --color-hx-up: #3fc380;
  --color-hx-red: #e84057;
  --color-hx-keystone-border: #463f2e;
  --font-hx-serif: "Beaufort for LOL", "Cinzel", Georgia, serif;
}
```

   これで `bg-hx-bg-raised` / `text-hx-gold` / `border-hx-border` / `font-hx-serif` 等が使える。
2. **Preflight 衝突確認:** Tailwind の preflight(リセット CSS)が旧 styles.css と同居した状態で `pnpm tauri dev` + モックモードを起動し、表示崩れがないか確認。崩れたら該当箇所だけ旧 CSS の specificity を上げるのではなく、その場でそのパネルのユーティリティ化を前倒しして解消する。特に `img`(preflight で `display: block; max-width: 100%; height: auto`)とボタンのデフォルトスタイル除去に注意。
3. パネル単位で変換する。順番: StatusChip → LpBanner/RuneBanner → SettingsPanel → InGamePanel → HEXGATE 一式。1パネル変換するごとに styles.css から対応ブロックを削除し、モックモードで見比べる。
4. **ユーティリティに変換するもの:** レイアウト(flex/grid/gap)、余白、サイズ、色、フォント、角丸、ボーダー、単純な hover 状態。サイズは既存 px 値を維持(スケールに合えば `w-7`、合わなければ `w-[28px]` の arbitrary value)。
5. **カスタム CSS として app.css に残すもの**(無理にユーティリティ化しない):
   - `html, body` の base 宣言(`@layer base`): transparent 背景、`pointer-events: none`、`overflow: hidden`、`user-select: none`、フォントスタック。
   - `.panel` の共通装飾(ゴールドのグラデーションオーバーレイ + backdrop-filter + シャドウ)— `@layer components` のクラスとして1個維持してよい。
   - 折りたたみの `grid-template-rows` transition 一式。
   - `hx-shimmer` keyframes とスケルトン系。
   - `body.interactive::after` のゴールド枠オーバーレイ。
   - SVG の stroke/fill 指定など、セレクタ構造が深いもの。
6. styles.css が空になったら削除し、`index.html` のリンクも外す。

**チェックポイント:** `pnpm build` が通る。モックモードで§10 を再度全確認。styles.css が消えている。

### フェーズ 4: 仕上げ

1. 未使用コード・コメントの掃除。`.hidden` ユーティリティ(`display:none !important`)が不要になっていること(表示制御は `<Show>` に統一。例外は折りたたみのみ)。
2. `pnpm build` 最終確認。
3. 変更ファイル一覧と、人間が Windows 実機で確認すべき項目(§10 の「実機確認」)を最終報告にまとめる。

---

## §7 状態マッピング表(旧 → 新)

### main.ts → state/ + components/

| 旧 | 新 |
|---|---|
| `let wasInGame` + `renderPhase` | `phase` シグナル(`PhaseEvent`)。in-game→非in-game の遷移で recs パネルを隠すのは `createEffect` で前回値を比較 |
| `lastRecs` / `lastSummoner` / `lastHistory` + initAssets 後の再レンダー | それぞれシグナル。assets 解決は `assetsReady` シグナル(後述)で自動再評価されるので「再レンダー呼び直し」は不要になる |
| `runeBannerTimer` / `lpBannerTimer` | 各バナーコンポーネント内のシグナル + `setTimeout`(再表示時は `clearTimeout` してから張り直す。旧コードと同じ 6s / 12s) |
| `currentWindowMode` | `windowMode` シグナル |
| `reportHitRegions` + interval | `lib/hitRegions.ts` にそのまま移植(ロジック変更なし) |
| `initPanelDragHandle` / `initWindowDragHandles` | `lib/drag.ts`。コンポーネントからは `ref` で要素を渡して呼ぶ |
| `applyUiLayout` / `applyIngameCollapsed` | `state/layout.ts`: `ingamePos` シグナル + `collapsed` シグナル。位置は `style` バインドではなく**従来どおり直接 style 書き換えでもよい**(ドラッグ中の 60fps 更新をシグナル経由にしない方が素直) |
| settings パネルの表示制御(gear クリック + `interactive` イベント) | `settingsOpen` シグナル。gear クリックでトグル、`interactive` イベントで強制 set。`interactive` は `document.body.classList.toggle("interactive", on)` も維持(createEffect で) |

### champselect.ts → state/ + components/hexgate/

| 旧 | 新 |
|---|---|
| `let cs` | `champSelect` シグナル(`ChampSelectEvent \| null`) |
| `windowMode`, `pinned`, `importSpells`, `spellsFlipped`, `selectedRole`, `activeTab`, `vsEnemyId`, `userPickedVsEnemy`, `hoverChampId`, `importState` | すべて個別シグナル。`settings.ts` に属するもの(importSpells/spellsFlipped/pinned)はそちらへ |
| `assetsVersion` カウンタ | `assets.ts` に `assetsReady: () => boolean` シグナルを追加(`initAssets` 完了で true)。`champName()` 等を呼ぶ JSX/メモの先頭で `assetsReady()` を読んでおけば、解決時に自動で再評価される |
| `scheduleRender` / `renderAll` / render-key guards (`listsKey` 等) | **全削除。** Solid の依存追跡が代替する。guard が守っていた「無関係な再レンダーで hover リスナーや img が壊れない」性質は、`<For>`/`<Index>` のノード再利用が保証する |
| `makeCache` | `state/caches.ts` のシグナル版(§8.3)。`invalidate + scheduleRender` のリトライは `refetch(key)` に置き換え |
| `effectiveRole()`, `firstEnemy()`, `revealedEnemies()`, `displayedTarget()` | そのまま関数 or `createMemo`。シグナルを読むだけで自動的にリアクティブになる |
| `onChampSelect` の VS ターゲット追従ロジック | `champ-select` イベントハンドラ内に**そのまま**移植(挙動を変えない: 手動選択は盤面に残る限り維持、消えたら先頭の敵に戻す、敵ゼロで vs タブなら best へ) |
| リスト再構築時の `hoverChampId = 0` リセット | `createEffect` で tier リストの入力(role / entry state / ban 集合)の変化を監視してクリア。`cs.active` が false になった時もクリア(旧コードと同じ) |
| `renderImportButton` + `finishImport` | `importState` シグナル + ボタンコンポーネント。importing 中 disabled、imported は 2s / failed は 3s で idle に戻す。再クリック時の `clearTimeout` も維持 |
| 設定パネルの `#spells-import` と stats 行の `#hx-spells-on` の相互同期 | 単一の `importSpells` シグナルに両チェックボックスをバインドするだけで解消 |
| VS メニューの外側クリックで閉じる(document click) | `onMount` で document に listener、`onCleanup` で解除 |
| `phaseChipLabel` | そのままユーティリティ関数として移植 |

### 表示制御の変換規則

- 旧 `.hidden` クラスのトグル → 原則 `<Show when={...}>`(アンマウント)。
- **例外:** in-game パネルの折りたたみ(§2-5)はマウントしたままクラス切替。
- スキルアイコンの `visibility: hidden`(ロード完了まで非表示) → 旧挙動のまま(レイアウト保持のため `<Show>` にしない)。

---

## §8 Solid 実装ガイド(担当モデルが踏みがちな罠)

### 8.1 基本ルール

- **コンポーネント関数は1回しか実行されない。** React と違い再実行されない。条件分岐は JSX 内の `<Show>`/`<Switch>`、リストは `<For>`(オブジェクト配列)/`<Index>`(プリミティブ配列: skill id 列、敵 champion id 列など)。
- **props を分割代入しない**(リアクティビティが切れる)。`props.foo` のまま使う。
- `createEffect` 内で読んだシグナルだけが追跡される。非同期コールバック内の読み取りは追跡されない。
- クラスの条件付与は `classList={{ active: cond() }}` または `class={...}` の文字列組み立て。
- イベントは `onClick` / `onMouseEnter` 等。`document` / `window` へのリスナーは `onMount` + `onCleanup` で管理。

### 8.2 イベント → シグナル(state/backend.ts)

```ts
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import type { PhaseEvent /* … */ } from "../types";

const [phase, setPhase] = createSignal<PhaseEvent | null>(null);
export { phase };

// モジュール初期化で一度だけ張る。listen の解除は不要(ウィンドウは生き続ける)。
listen<PhaseEvent>("phase", (e) => setPhase(e.payload));
```

全イベントを同じ形で並べる。`log` イベントは従来どおり console.log に流すだけ。

### 8.3 makeCache のシグナル版(state/caches.ts)

```ts
import { createSignal, type Accessor } from "solid-js";

type CacheEntry<T> =
  | { state: "loading" }
  | { state: "ok"; value: T }
  | { state: "err"; error: string };

export function makeCache<T>(fetcher: (key: string) => Promise<T>) {
  const map = new Map<string, {
    entry: Accessor<CacheEntry<T>>;
    set: (e: CacheEntry<T>) => void;
  }>();

  const fire = (key: string, set: (e: CacheEntry<T>) => void) => {
    fetcher(key).then(
      (value) => set({ state: "ok", value }),
      (err) => set({ state: "err", error: errorMessage(err) }),
    );
  };

  return {
    /** リアクティブ読み取り。初回アクセスで fetch が走る。 */
    get(key: string): CacheEntry<T> {
      let slot = map.get(key);
      if (!slot) {
        const [entry, setEntry] = createSignal<CacheEntry<T>>({ state: "loading" });
        slot = { entry, set: setEntry };
        map.set(key, slot);
        fire(key, setEntry);
      }
      return slot.entry();
    },
    /** リトライ用: 同じシグナルを loading に戻して再 fetch(リアクティビティ維持)。 */
    refetch(key: string) {
      const slot = map.get(key);
      if (!slot) return;
      slot.set({ state: "loading" });
      fire(key, slot.set);
    },
  };
}
```

`errorMessage` は champselect.ts の実装をそのまま移植。`tierCache` / `counterCache` / `buildCache` のキー形式(`role`, `enemy|role`, `champ|role|enemy`)と `"not-enough-data"` エラーの特別扱いも現状どおり。

### 8.4 Icon コンポーネント(components/Icon.tsx)

`assets.ts` の `setIcon` は `<img>` に非同期でアイコンを流し込むヘルパー。URL がシグナル由来で変わる場所があるため、ref + createEffect でラップする:

```tsx
import { createEffect } from "solid-js";
import { setIcon } from "../assets";

export function Icon(props: { url: string; class?: string; title?: string; alt?: string }) {
  let el!: HTMLImageElement;
  createEffect(() => setIcon(el, props.url));
  return <img ref={el} class={props.class} title={props.title} alt={props.alt ?? ""} />;
}
```

旧コードで `document.createElement("img")` + `setIcon` していた箇所はすべてこれに置き換える。`getAbility`(スキルアイコンの非同期解決)は SkillOrder コンポーネント内でシグナルに受けて解決する。

### 8.5 ドラッグとレイアウトの直接 DOM 操作

ドラッグ中の `style.left/top` 書き換えはシグナルを経由させず、旧コードのまま `ref` で得た要素に直接書く(60fps の更新にリアクティブシステムを噛ませる理由がない)。永続化(`set_ingame_panel_position` 等)とヒット領域報告のタイミングだけ §2-4 を厳守。

---

## §9 Tailwind 実装ガイド

- **設定ファイルなし。** v4 は CSS 内 `@theme` がコンフィグ(§6 フェーズ3の app.css 参照)。`tailwind.config.js` を作らない。
- 既存デザイントークン(`--hx-*`)は `@theme` の `--color-hx-*` / `--font-hx-serif` に移す。旧 CSS の「Legacy aliases」(`--bg`, `--good` 等)は変換時に `hx-*` 系へ統一して廃止する。
- px 値の維持を優先。Tailwind スケールに一致する値(4の倍数系)はスケールを、それ以外は `w-[448px]` のような arbitrary value を使う。**デザイン上の寸法を「キリが良いから」と変えない。**
- 状態依存スタイル(`.dot.ingame`, `.threat-chip.ad`, `.form-game.win` 等)は、クラス名分岐をやめて JSX 側で `classList` によりユーティリティを出し分ける形に変換してよい(例: `classList={{ "bg-hx-up": inGame(), "bg-hx-red": !clientUp() }}`)。
- 残すカスタム CSS は §6 フェーズ3-5 のリストが上限。迷ったらユーティリティ化を試み、3個以上のパネルで共有される装飾だけ `@layer components` に置く。

---

## §10 検証

### ビルド検証(全フェーズ共通)

```bash
pnpm build        # tsc(strict) + vite build。これが唯一の自動チェック。
```

JS のテストランナーは存在しない。Rust テストは無関係(触っていないので実行不要)。

### モックモードによる動作検証(Mac で可能)

`pnpm tauri dev` で起動し **Ctrl+Shift+D** でモックモードをトグルする。`mock.rs` が champ-select シーン → in-game シーンを実データ(provider 経由)で順に流すので、両パネルを実機なしで確認できる。

**チェックリスト(フェーズ2と3の完了時に全項目):**

- [ ] ステータスチップ: 接続状態ドットの色、プロフィール(名前/ランク/勝率)、直近戦績ストリップ(勝敗ボーダー色、連勝/連敗表示)
- [ ] HEXGATE: champ-select 突入でパネル表示、phase チップ更新
- [ ] HEXGATE: STRONG PICKS / BAN TARGETS のスケルトン → データ表示、WR/Δ/games 列
- [ ] HEXGATE: 行ホバーでビルドエリアがプレビューに切り替わり、ホバー解除で自分のピックに戻る
- [ ] HEXGATE: ロールチップ(ロール未確定時のみ表示)切替でリスト更新
- [ ] HEXGATE: 敵スロット表示、カウンターストリップ、カウンターホバーのプレビュー
- [ ] HEXGATE: BEST/VS タブ切替、VS ドロップダウンで対象変更、外側クリックで閉じる
- [ ] HEXGATE: ルーンページ(ツリーヘッダー色、キーストーン、シャード)、stats 行(WR/games/スペル)、FLIP でスペル順反転
- [ ] HEXGATE: IMPORT ボタンの idle→importing→imported/failed 遷移と文言
- [ ] HEXGATE: pin トグルで champ-select 終了後もパネル維持
- [ ] HEXGATE: ヘッダードラッグでウィンドウ移動
- [ ] 設定パネル: gear で開閉、チェックボックス2か所(設定パネル/stats 行)の同期、Ctrl+Shift+O で強制表示+ゴールド枠
- [ ] in-game パネル: 推奨アイテム(アイコン/名前/理由/スコアバー)、敵アイコン、脅威チップ(AD/AP/TANK/CC)、スキルオーダー
- [ ] in-game パネル: ヘッダードラッグで移動 → 位置が再起動後も復元、シェブロンで折りたたみ(width アニメーション)→ 状態が復元
- [ ] ルーンバナー(6s)・LP バナー(12s)の表示と自動消滅
- [ ] エラー系: ネットワーク断で tier list がエラー表示 → Retry で再取得(DevTools の offline 等で確認)

### 実機確認(人間が Windows で行う — 最終報告に明記すること)

- クリックスルー: ヒット領域外のクリックがゲームに通る/ヘッダー類はクリックできる
- ルーン/スペルの実インポート(`import_build`)
- Ctrl+Shift+M のモニター移動後のヒット領域整合

---

## §11 触ってはいけないもの(再掲・最終確認用)

- `src-tauri/` 以下すべて
- `src/types.ts` の型定義(import して使うのは当然 OK)
- `tauri.conf.json`
- イベント名・コマンド名・ペイロード形状
- Google Fonts(Cinzel)のリンク
- `data-hit` の付与対象、`pointer-events` の構造、ドラッグ/折りたたみ/ヒット領域の挙動

## §12 Definition of Done

1. `pnpm build` が警告・エラーなしで通る。
2. `src/main.ts`, `src/champselect.ts`, `src/styles.css` が削除され、UI はすべて Solid コンポーネント + Tailwind ユーティリティ(+§6 フェーズ3-5 の許容カスタム CSS)になっている。
3. §10 のモックモードチェックリストが全項目パス。
4. 最終報告に: 変更ファイル一覧、残したカスタム CSS とその理由、Windows 実機確認が必要な項目リスト。
