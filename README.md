# lol-overlay

軽量 League of Legends オーバーレイ。Tauri (Rust + WebView) 製。
ゲームプロセスへのインジェクション無し・公式ローカル API のみで動作する。

## 何ができるか

- **試合中のアイテム推奨** — Live Client Data API で敵構成を読み、敵のダメージ
  タイプ(AD / AP / タンク)に応じた防具などを透明オーバーレイに表示。
- **チャンピオン選択でのルーン自動インポート** — LCU API で自分のピックを検知し、
  推奨ルーンページをクライアントへ書き込む。

## アーキテクチャ

```
src-tauri/src/
  live_client.rs   Live Client Data API (127.0.0.1:2999) — 試合中の状態を読む
  lcu.rs           LCU access (irelia crate) — phase / champ select / runes / WS
  provider/        データ源の抽象 (BuildProvider trait)
    mod.rs           trait + 共通型 + 脅威分類ヒューリスティック
    hardcoded.rs     スタブ実装(後で実データに差し替える）
  lib.rs           エンジン:WS + rune processor + poller + commands/events
src/
  main.ts          イベント購読 → オーバーレイ描画
```

LCU は [`irelia`](https://github.com/AlsoSylv/Irelia) クレート経由で、lockfile の
探索・認証・自己署名証明書を任せている。オーケストレーションは3要素:

- **WebSocket** — champ-select セッション更新を購読し、チャンネルへ流す(即時)。
- **rune processor** — チャンネルを drain し、ピック変化時にルーンをインポート。
- **poller** — 2秒間隔で phase / in-game 状態を取得して UI に反映(かつ champ
  select 中はセッションを REST で取り直してチャンネルへ流す取りこぼし対策)。

`phase` / `recommendations` / `rune-imported` / `log` イベントをフロントが描画する。
in-game(アイテム推奨)は Live Client Data API に WebSocket が無いため polling。

## オーバーレイの仕組み(重要)

LoL を **Borderless(ボーダーレス)モード**で起動すること。本アプリは透明・
最前面・クリックスルーの OS ウィンドウをゲームに重ねるだけで、ゲームには一切
注入しない。排他的フルスクリーンではこの方式ではオーバーレイが見えない。

## 開発・ビルド

ターゲットは **Windows**。Mac では UI 開発はできるが、LCU / Live Client API は
LoL クライアントが必要なため実機検証は Windows で行う。

```bash
pnpm install
pnpm tauri dev      # 開発
pnpm tauri build    # 配布ビルド (Windows 上で実行)
```

lockfile の探索・認証は `irelia` が自動で行うため、環境変数の設定は不要。

## データ源の差し替え(次のステップ)

`provider/hardcoded.rs` はプレースホルダ。実データを使うには
`BuildProvider` を実装した型を作り、`lib.rs` の `run()` 内
`provider: Arc::new(HardcodedProvider)` を差し替えるだけ。候補:

- Riot **Match-V5 API** から自前で勝率/ピック率を集計した DB
- 公開データセット / スクレイピング
- AI モデルによる推奨

## ⚠️ Riot ToS について

- 公式 API の読み取りとボーダーレス重ね描画は ToS 準拠(メモリ読取・注入はしない)。
- ただし **ルーン書き込み等 LCU を使うアプリは公開前に Riot への登録・審査が必須**。
  Developer Portal で申請すること: <https://developer.riotgames.com/>
