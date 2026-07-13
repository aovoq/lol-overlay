# U.GG Player Stats 内部 API 調査

調査日: 2026-07-14 (JST)

対象:

- `https://u.gg/lol/profile/kr/hide%20on%20bush-kr1/overview`
- 実 Chrome のログアウト状態
- 調査時の U.GG Web build: `x-app-version: 459dec7dd477af342e49c5b736d46b351dac39a4`

秘密情報について、Cookie、Authorization の値、Cloudflare/Turnstile token は取得結果へ表示・保存していない。更新ボタンも、サーバー側の更新を起こし得るため実行していない。

## 結論

内部 JSON API の契約自体は存在する。エンドポイントは `POST https://u.gg/api` の GraphQL で、プロフィール、ランク、戦績、Champion Stats の query が Web bundle 内に定義されている。

ただし、Rust `reqwest` からの安定した匿名直接取得方法は確認できなかった。Cookie を一切持たない直接 HTTP リクエストは、通常のアプリ header を付けても Cloudflare の `403 text/html` になった。実 Chrome でも、SSR 後のクライアント GraphQL が HTML を JSON として parse して失敗し、Match History は表示されなかった。

分類は、全データを対象にすると **E. 安定した取得方法なし**。

- **A/B ではない**: Cookie なしの `/api` は、`Content-Type` のみ、U.GG の `x-app-*` header 付き、通常の `Accept`/`Origin`/`Referer` 付きのいずれも Cloudflare 403。
- **C は可能性が高いが未確定**: Cloudflare 検証済み Cookie の値は扱っておらず、Cookie が唯一の必要条件であることまでは確認できない。今回の実 Chrome でもクライアント GraphQL は安定して成功しなかった。
- **D はデータ源として確認**: 初期 HTML に `window.__APOLLO_STATE__` があり、profile、rank、historic rank、overall rank、champion stats が埋め込まれる。ただしプロフィール HTML 自体も Cookie なしの直接 GET では Cloudflare 403 のため、匿名 reqwest の解決策にはならない。
- **matches は取得不能**: HTML 埋め込みに戦績はなく、クライアント GraphQL の page 1 が失敗した。

## 実 Chrome での観測結果

Turnstile の可視 challenge は表示されず、プロフィールの SSR 表示には成功した。表示された範囲はプロフィール、ランク、過去ランク、Ladder Rank、Champion Stats。Match History は見出しと filter のみで、match card は 0 件だった。

ページの resource inventory では、初回に `https://u.gg/api` への `fetch` が 14 回記録された。再読込後は 3 回記録された。再読込のたびに U.GG の main bundle から次のエラーが 2 回記録された。

```text
SyntaxError: Unexpected token '<', "<html>..." is not valid JSON
```

これは GraphQL client が JSON ではなく HTML を受けたことを示す。Chrome 制御面から response status/body を直接読めないため、その HTML が Cloudflare challenge であることは Chrome 内では断定していない。ただし、後述の Cookie なし直接 HTTP では同じ endpoint が Cloudflare challenge HTML を返した。

初回 page が取得できないため、スクロールによる追加読込を成功状態では観測できなかった。pagination の仕組みは Web bundle から確認した。

## HTTP 契約

### Endpoint

| 項目 | 値 |
|---|---|
| Host | `u.gg` |
| Path | `/api` |
| Method | `POST` |
| Protocol | GraphQL over JSON |
| Public API link | `https://u.gg/api` |
| Auth API link | `https://auth.u.gg/api`（今回の Player Stats 取得には不使用） |

Apollo client は public operation を `https://u.gg/api` へ送る。ログアウト状態でも public link に Authorization link が入り、空の `authorization` header を組み立てる実装になっているが、値は記録していない。

### Web client が設定する header 名

```text
content-type: application/json
x-app-type: web
x-app-version: 459dec7dd477af342e49c5b736d46b351dac39a4
authorization: <empty when logged out; value not recorded>
```

ブラウザはこれに `Origin`、`Referer`、`User-Agent`、`Accept`、Cookie 等を加える。`x-app-version` は deploy ごとに変わる build identifier なので、固定値を安定 API key と見なすべきではない。

Cloudflare 403 が先に発生するため、`x-app-type` と `x-app-version` が GraphQL application layer で必須かは分離確認できなかった。

### JSON request envelope

```json
{
  "operationName": "getSummonerProfile",
  "variables": {
    "regionId": "kr",
    "seasonId": 26,
    "riotUserName": "hide on bush",
    "riotTagLine": "kr1"
  },
  "query": "query getSummonerProfile(...) { ... }"
}
```

Apollo の標準 GraphQL JSON envelope で、`operationName`、`variables`、`query` を送る。

## 取得元と operation

| データ | operationName | Root field | 初期 HTML 埋め込み | Client fetch |
|---|---|---|---|---|
| Profile + current ranks | `getSummonerProfile` | `profileInitSimple`, `fetchProfileRanks` | あり | hydration/refetch あり |
| Ladder rank | `getPlayerOverallRanking` | `overallRanking` | あり | あり |
| Historic ranks | `historicRanks` | `getHistoricRanks` | あり | あり |
| Champion Stats | `getPlayerStats` | `fetchPlayerStatistics` | あり | filter/refetch あり |
| Match history | `FetchMatchSummaries` | `fetchPlayerMatchSummaries` | なし | あり、今回失敗 |
| Rank snapshots for match LP | `getSummonerRankSnapshots` | `getSummonerRankSnapshots` | なし | あり、今回失敗 |
| Live-game badge | `LiveGameExists` | `liveGameExists` | なし | あり |
| Profile refresh | `UpdatePlayerProfile` | `updatePlayerProfile` | なし | 未実行 |

## Query と variables

### Profile + rank: `getSummonerProfile`

```graphql
query getSummonerProfile(
  $regionId: String!
  $seasonId: Int!
  $riotUserName: String!
  $riotTagLine: String!
) {
  fetchProfileRanks(
    riotUserName: $riotUserName
    riotTagLine: $riotTagLine
    regionId: $regionId
    seasonId: $seasonId
  ) {
    rankScores {
      lastUpdatedAt
      losses
      lp
      promoProgress
      queueType
      rank
      role
      seasonId
      tier
      wins
    }
  }
  profileInitSimple(
    regionId: $regionId
    riotUserName: $riotUserName
    riotTagLine: $riotTagLine
  ) {
    lastModified
    memberStatus
    playerInfo {
      accountIdV3
      accountIdV4
      exodiaUuid
      iconId
      puuidV4
      regionId
      summonerIdV3
      summonerIdV4
      summonerLevel
      riotUserName
      riotTagLine
    }
    premium
    customizationData {
      headerBg
      twitchName
      twitterName
      youtubeName
    }
  }
}
```

```json
{
  "regionId": "kr",
  "seasonId": 26,
  "riotUserName": "hide on bush",
  "riotTagLine": "kr1"
}
```

主要 response shape:

```text
data
├─ profileInitSimple
│  ├─ lastModified: string
│  ├─ memberStatus: string
│  ├─ premium: boolean
│  ├─ customizationData: object | null
│  └─ playerInfo
│     ├─ iconId: number
│     ├─ regionId: string
│     ├─ summonerLevel: number
│     ├─ riotUserName: string
│     ├─ riotTagLine: string
│     └─ account/puuid/summoner/exodia identifiers
└─ fetchProfileRanks
   └─ rankScores[]
      ├─ lastUpdatedAt: number
      ├─ wins/losses/lp: number
      ├─ queueType/rank/role/tier: string
      └─ seasonId: number
```

### Champion Stats: `getPlayerStats`

```graphql
query getPlayerStats(
  $queueType: [Int!]
  $regionId: String!
  $role: Int!
  $seasonId: Int!
  $riotUserName: String!
  $riotTagLine: String!
) {
  fetchPlayerStatistics(
    queueType: $queueType
    riotUserName: $riotUserName
    riotTagLine: $riotTagLine
    regionId: $regionId
    role: $role
    seasonId: $seasonId
  ) {
    basicChampionPerformances {
      assists
      championId
      cs
      damage
      damageTaken
      deaths
      doubleKills
      gold
      kills
      maxDeaths
      maxKills
      pentaKills
      quadraKills
      totalMatches
      tripleKills
      wins
      lpAvg
      firstPlace
      totalPlacement
    }
    exodiaUuid
    puuid
    queueType
    regionId
    role
    seasonId
  }
}
```

Overview の variables:

```json
{
  "queueType": [420, 440],
  "regionId": "kr",
  "role": 7,
  "seasonId": 26,
  "riotUserName": "hide on bush",
  "riotTagLine": "kr1"
}
```

埋め込み response では `fetchPlayerStatistics` は queue ごとの配列だった。今回の例は 2 要素で、各要素に `basicChampionPerformances[]`、`queueType`、`regionId`、`role`、`seasonId` と識別子が入る。各 champion 要素は query に列挙した集計値を持つ。

### Match history: `FetchMatchSummaries`

```graphql
query FetchMatchSummaries(
  $championId: [Int]
  $page: Int
  $queueType: [Int]
  $duoRiotUserName: String
  $duoRiotTagLine: String
  $regionId: String!
  $role: [Int]
  $seasonIds: [Int]!
  $riotUserName: String!
  $riotTagLine: String!
) {
  fetchPlayerMatchSummaries(
    championId: $championId
    page: $page
    queueType: $queueType
    duoRiotUserName: $duoRiotUserName
    duoRiotTagLine: $duoRiotTagLine
    regionId: $regionId
    role: $role
    seasonIds: $seasonIds
    riotUserName: $riotUserName
    riotTagLine: $riotTagLine
  ) {
    finishedMatchSummaries
    totalNumMatches
    matchSummaries {
      assists
      augments
      championId
      cs
      damage
      deaths
      gold
      items
      jungleCs
      killParticipation
      kills
      level
      matchCreationTime
      matchDuration
      matchId
      maximumKillStreak
      primaryStyle
      queueType
      regionId
      role
      runes
      subStyle
      summonerName
      riotUserName
      riotTagLine
      summonerSpells
      psHardCarry
      psTeamPlay
      lpInfo {
        lp
        placement
        promoProgress
        promoTarget
        promotedTo { tier rank }
      }
      teamA {
        championId summonerName riotUserName riotTagLine teamId role
        hardCarry teamplay placement playerSubteamId
      }
      teamB {
        championId summonerName riotUserName riotTagLine teamId role
        hardCarry teamplay placement playerSubteamId
      }
      version
      visionScore
      win
      roleQuestCompletion
      roleBoundItem
    }
  }
}
```

All Matches の初期 variables は bundle の生成ロジック上、次の形になる。

```json
{
  "regionId": "kr",
  "riotUserName": "hide on bush",
  "riotTagLine": "kr1",
  "queueType": [],
  "duoRiotUserName": "",
  "duoRiotTagLine": "",
  "role": [],
  "seasonIds": [26, 25],
  "championId": [],
  "page": 1
}
```

Pagination は cursor ではなく 1-based の `page`。画面下端の 100 px 手前へ到達すると `page` を 1 増やして `fetchMore` し、`matchId` で重複排除して末尾へ連結する。終了判定は response の `finishedMatchSummaries`。

`pageSize` parameter はない。サーバー固定件数で、今回 page 1 response を取得できなかったため、**1 page = 20 件であることは実測確認できなかった**。20 件固定として実装するより、`finishedMatchSummaries` と実際の `matchSummaries.length` を使う必要がある。

### 補助 rank operation

```text
getPlayerOverallRanking variables:
  { queueType: 420, regionId: "kr", riotUserName: "hide on bush", riotTagLine: "kr1" }
response:
  data.overallRanking { overallRanking: number, totalPlayerCount: number }

historicRanks variables:
  { queueType: 420, regionId: "kr", riotUserName: "hide on bush", riotTagLine: "kr1" }
response:
  data.getHistoricRanks[] { lp, queueId, rank, regionId, season, tier }

getSummonerRankSnapshots variables:
  { queueType: [420, 440], regionId: "kr", riotUserName: "hide on bush", riotTagLine: "kr1" }
response selection:
  data.getSummonerRankSnapshots[] { insertedAt, losses, lp, promoProgress, queueId, rank, tier, wins }
```

### Update: `UpdatePlayerProfile`

```graphql
query UpdatePlayerProfile(
  $regionId: String!
  $riotUserName: String!
  $riotTagLine: String!
) {
  updatePlayerProfile(
    regionId: $regionId
    riotUserName: $riotUserName
    riotTagLine: $riotTagLine
  ) {
    success
    errorReason
  }
}
```

variables は `{ regionId, riotUserName, riotTagLine }`。GraphQL 上は `query` だが、名前と UI 動作からサーバー側 refresh を起こし得る。データ変更操作禁止に従い、クリック・送信は行っていない。成功後に profile/rank/matches/champion stats の登録済み refetch を走らせる実装だった。

## HTML 埋め込み JSON

SSR HTML の inline script に次が存在する。

```js
window.__APOLLO_STATE__ = {
  "ROOT_QUERY": {
    "overallRanking({...})": { "...": "..." },
    "getHistoricRanks({...})": ["..."],
    "fetchProfileRanks({...})": { "...": "..." },
    "profileInitSimple({...})": { "...": "..." },
    "fetchPlayerStatistics({...})": ["..."]
  }
}
```

この例では root field の argument key に `regionId`, `riotUserName`, `riotTagLine`, `seasonId`, `queueType`, `role` が JSON 文字列として埋め込まれていた。値は GraphQL response と同じ shape で nested object として格納される。Apollo normalized entity cache ではなく、観測時は `ROOT_QUERY` 1 object の nested data だった。

Match History の `fetchPlayerMatchSummaries` は埋め込まれていなかった。

また、Cookie なし直接 GET で対象プロフィール HTML を取得すると `403 text/html` だった。このため、`reqwest` で HTML を取得して `__APOLLO_STATE__` を parse する方法も匿名では再現できない。

## Cookie なし検証

秘密値を使わず、同じ GraphQL body を Cookie なしで `POST https://u.gg/api` へ送った結果:

| Header 条件 | Status | Content-Type | 結果 |
|---|---:|---|---|
| `content-type: application/json` のみ | 403 | `text/html; charset=UTF-8` | Cloudflare challenge HTML |
| 上記 + `x-app-type`, `x-app-version` | 403 | `text/html; charset=UTF-8` | Cloudflare challenge HTML |
| 上記 + `Accept`, `Origin`, `Referer`, 説明的 User-Agent | 403 | `text/html; charset=UTF-8` | Cloudflare challenge HTML |

ブラウザ内 `credentials: omit` の厳密な実行について、使用した Chrome 制御面の read-only page evaluator では `fetch` が提供されていなかった。URL 経由の script 実行はブラウザのセキュリティポリシーで拒否されたため、回避・別 surface での注入は行っていない。

従って、次の 2 点を分けて扱う必要がある。

1. **厳密な同一ページ内 `credentials: omit` は未実行**。
2. **reqwest 相当の Cookie なし直接 HTTP は実行済みで、Cloudflare 403**。

Rust 移植可否の判定では 2 が直接的な否定材料になる。少なくとも匿名 reqwest + 通常 header だけでは再現できない。

## Rust provider への含意

- 現時点で U.GG GraphQL を production provider の安定 API として直接採用しない。
- `x-app-version` と query document は Web deploy で変わり得る非公開契約。
- Cloudflare Cookie/token を持ち出す方式は採用しない。
- HTML fallback も匿名 GET が 403 のため採用しない。
- 将来再調査する場合は、U.GG が明示的な public API を提供するか、Cookie なしの `/api` が正式に許可されることを条件にする。
- Match pagination を実装するとしても 20 件を定数化せず、`page`、`finishedMatchSummaries`、返却件数で制御する。

## 確認できなかった点

- 成功した `FetchMatchSummaries` response と実測 page size。
- 成功した追加読込時の Network response。
- Update 実行時の response（副作用禁止のため未実行）。
- 同一ページ JavaScript context での厳密な `credentials: omit`（Chrome 制御面の安全制約により未実行）。
- Cloudflare 検証済み Cookie だけで `/api` が安定して成功するか。Cookie/token を表示・保存・移送していないため未確認。
