import { A, useParams } from "@solidjs/router";
import { createMemo, createSignal, For, Show } from "solid-js";
import {
  allChampions,
  assetsReady,
  champIconByKey,
  champName,
  fmtPct,
  itemIconUrl,
  profileIconUrl,
} from "../../assets";
import { matchRank, normalizeForSearch, searchChampions } from "../../lib/championSearch";
import { fmtTier, ROLES, roleLabel } from "../../lib/openlol";
import { formSummary } from "../../lib/recentForm";
import { matchHistory, phase, selectedRole, setSelectedRole, summoner } from "../../state/backend";
import { buildDetailsCache, buildKey, tierCache } from "../../state/caches";
import { developerMode } from "../../state/settings";
import { DebugPanel } from "../DebugPanel";
import { Icon } from "../Icon";
import { InGamePanel } from "../ingame/InGamePanel";
import { SkillOrder } from "../ingame/SkillOrder";
import { BuildArea } from "../openlol/BuildArea";
import { Counters } from "../openlol/Counters";
import { ImportButton } from "../openlol/ImportButton";
import { StatsRow } from "../openlol/StatsRow";
import { ScrollArea } from "../ScrollArea";
import { SettingsForm } from "../SettingsPanel";

function PageHeader(props: { eyebrow?: string; title: string; description?: string }) {
  return (
    <header class="desktop-page-header">
      <Show when={props.eyebrow}>
        <span class="desktop-eyebrow">{props.eyebrow}</span>
      </Show>
      <h1>{props.title}</h1>
      <Show when={props.description}>
        <p>{props.description}</p>
      </Show>
    </header>
  );
}

export function HomePage() {
  const s = () => summoner();
  const games = () => matchHistory() ?? [];
  const winRate = createMemo(() => {
    const value = s();
    if (!value || value.soloWins + value.soloLosses === 0) return "—";
    return `${Math.round((value.soloWins / (value.soloWins + value.soloLosses)) * 100)}%`;
  });
  const form = createMemo(() => formSummary(games()));

  return (
    <ScrollArea class="h-full" contentClass="desktop-page">
      <PageHeader
        eyebrow="DASHBOARD"
        title="ホーム"
        description="ランクと最近の戦績をひと目で確認できます。"
      />
      <Show
        when={s()}
        fallback={
          <section class="desktop-card desktop-empty">
            Leagueクライアントの起動を待っています…
          </section>
        }
      >
        {(player) => (
          <section class="desktop-profile-card desktop-card">
            <Icon url={profileIconUrl(player().profileIconId)} class="desktop-profile-icon" />
            <div class="desktop-profile-copy">
              <strong>
                {player().gameName} <span>#{player().tagLine}</span>
              </strong>
              <small>LEVEL {player().level}</small>
            </div>
            <div class="desktop-rank-stat">
              <span>SOLO RANK</span>
              <strong>
                {player().soloTier
                  ? `${fmtTier(player().soloTier)} ${player().soloDivision}`
                  : "Unranked"}
              </strong>
              <small>{player().soloTier ? `${player().soloLp} LP` : "—"}</small>
            </div>
            <div class="desktop-rank-stat">
              <span>WIN RATE</span>
              <strong>{winRate()}</strong>
              <small>
                {player().soloWins}W {player().soloLosses}L
              </small>
            </div>
          </section>
        )}
      </Show>

      <div class="desktop-section-heading">
        <h2>最近の試合</h2>
        <span>{games().length} GAMES</span>
      </div>
      <section class="desktop-card desktop-match-list">
        <Show when={form()}>
          {(f) => (
            <div class="desktop-form-row">
              <div class="desktop-form-strip">
                <Show when={assetsReady()}>
                  <For each={games()}>
                    {(g) => (
                      <Icon
                        url={champIconByKey(g.championId)}
                        class={g.win ? "is-win" : "is-loss"}
                        title={`${champName(g.championId)} · ${g.kills}/${g.deaths}/${g.assists} · ${g.win ? "勝利" : "敗北"}`}
                      />
                    )}
                  </For>
                </Show>
              </div>
              <span class="desktop-form-summary">
                {f().record} · {f().kda}
                <Show when={f().streakLabel}>
                  <span
                    class={`desktop-form-streak ${
                      f().streakWin ? "is-win" : f().streakLoss ? "is-loss" : ""
                    }`}
                  >
                    {f().streakLabel}
                  </span>
                </Show>
              </span>
            </div>
          )}
        </Show>
        <Show
          when={games().length > 0}
          fallback={<div class="desktop-empty">試合履歴はまだありません。</div>}
        >
          <For each={games()}>
            {(game) => (
              <div class={`desktop-match-row ${game.win ? "is-win" : "is-loss"}`}>
                <Icon url={champIconByKey(game.championId)} class="desktop-match-icon" />
                <div>
                  <strong>{champName(game.championId) || `#${game.championId}`}</strong>
                  <small>{game.win ? "勝利" : "敗北"}</small>
                </div>
                <span class="desktop-kda">
                  {game.kills} / {game.deaths} / {game.assists}
                </span>
              </div>
            )}
          </For>
        </Show>
      </section>
    </ScrollArea>
  );
}

export function RoleSelector() {
  return (
    <div class="desktop-role-tabs">
      <For each={ROLES}>
        {(role) => (
          <button
            type="button"
            class={selectedRole() === role.lcu ? "is-active" : ""}
            onClick={() => setSelectedRole(role.lcu)}
          >
            {role.label}
          </button>
        )}
      </For>
    </div>
  );
}

export function ChampionsPage() {
  type SortKey = "winRate" | "pickRate" | "banRate";
  const [query, setQuery] = createSignal("");
  const [sort, setSort] = createSignal<SortKey>("winRate");
  const entry = createMemo(() => tierCache.get(selectedRole()));
  const champions = createMemo(() => {
    const value = entry();
    if (value.state !== "ok") return [];
    const needle = normalizeForSearch(query().trim());
    const infoByKey = new Map(allChampions().map((champ) => [champ.key, champ]));
    return [...value.value]
      .filter((item) => {
        if (!needle) return true;
        const info = infoByKey.get(item.championId);
        return info !== undefined && matchRank(needle, info) >= 0;
      })
      .sort((a, b) => b[sort()] - a[sort()]);
  });

  return (
    <ScrollArea class="h-full" contentClass="desktop-page">
      <PageHeader
        eyebrow="EXPLORE"
        title="チャンピオン"
        description="ロール別の成績からチャンピオンを選択します。"
      />
      <RoleSelector />
      <div class="desktop-champion-tools">
        <input
          type="search"
          value={query()}
          placeholder="チャンピオンを検索"
          onInput={(event) => setQuery(event.currentTarget.value)}
        />
        <select value={sort()} onChange={(event) => setSort(event.currentTarget.value as SortKey)}>
          <option value="winRate">勝率順</option>
          <option value="pickRate">ピック率順</option>
          <option value="banRate">BAN率順</option>
        </select>
      </div>
      <section class="desktop-champion-grid">
        <Show
          when={entry().state !== "loading"}
          fallback={
            <div class="desktop-card desktop-empty">チャンピオンデータを読み込んでいます…</div>
          }
        >
          <Show
            when={entry().state !== "err"}
            fallback={
              <div class="desktop-card desktop-empty desktop-error-state">
                <span>チャンピオンデータを取得できませんでした。</span>
                <button type="button" onClick={() => tierCache.refetch(selectedRole())}>
                  再試行
                </button>
              </div>
            }
          >
            <For
              each={champions()}
              fallback={<div class="desktop-card desktop-empty">一致する結果がありません。</div>}
            >
              {(champion, index) => (
                <A href={`/champions/${champion.championId}`} class="desktop-champion-card">
                  <span class="desktop-champion-rank">{index() + 1}</span>
                  <Icon url={champIconByKey(champion.championId)} class="desktop-champion-icon" />
                  <div>
                    <strong>{champName(champion.championId)}</strong>
                    <small>{roleLabel(selectedRole())}</small>
                  </div>
                  <span class="desktop-champion-wr">
                    {fmtPct(champion[sort()])}
                    <small>
                      {sort() === "winRate"
                        ? "WIN RATE"
                        : sort() === "pickRate"
                          ? "PICK RATE"
                          : "BAN RATE"}
                    </small>
                  </span>
                </A>
              )}
            </For>
          </Show>
        </Show>
      </section>
    </ScrollArea>
  );
}

export function ChampionPage() {
  const params = useParams();
  const championId = createMemo(() => Number(params.id));
  const champion = createMemo(() => allChampions().find((item) => item.key === championId()));
  const [vsEnemyId, setVsEnemyId] = createSignal(0);
  const selectedEnemy = createMemo(() => vsEnemyId() || null);
  const [enemyQuery, setEnemyQuery] = createSignal("");
  const [enemyMenuOpen, setEnemyMenuOpen] = createSignal(false);
  const matchingEnemies = createMemo(() =>
    searchChampions(allChampions(), enemyQuery()).slice(0, 12),
  );
  const detailsKey = createMemo(() => buildKey(championId(), selectedRole(), selectedEnemy()));
  const details = createMemo(() => buildDetailsCache.get(detailsKey()));
  const detailsValue = createMemo(() => {
    const entry = details();
    return entry.state === "ok" ? entry.value : null;
  });
  const chooseEnemy = (enemyId: number) => {
    setVsEnemyId(enemyId);
    setEnemyQuery(enemyId ? champName(enemyId) : "");
    setEnemyMenuOpen(false);
  };

  return (
    <Show
      when={!assetsReady() || champion()}
      fallback={
        <div class="desktop-page">
          <section class="desktop-card desktop-empty desktop-error-state">
            <span>指定されたチャンピオンが見つかりません。</span>
            <A href="/champions">チャンピオン一覧へ戻る</A>
          </section>
        </div>
      }
    >
      <div class="desktop-page desktop-champion-detail">
        <div class="desktop-detail-header">
          <A href="/champions" class="desktop-back-link">
            ← CHAMPIONS
          </A>
          <Show when={assetsReady() && champion()}>
            <Icon url={champIconByKey(championId())} class="desktop-detail-icon" />
          </Show>
          <div>
            <span>{roleLabel(selectedRole())}</span>
            <h1>{champion()?.name || `Champion #${championId()}`}</h1>
          </div>
        </div>
        <RoleSelector />
        <div class="desktop-detail-grid">
          <aside class="desktop-detail-aside">
            <section class="desktop-card desktop-matchup-card">
              <div class="desktop-section-heading">
                <h2>対面</h2>
                <span>MATCHUP</span>
              </div>
              <div class="desktop-matchup-select">
                <span>対面チャンピオン</span>
                <input
                  type="search"
                  value={enemyQuery()}
                  placeholder={selectedEnemy() ? champName(selectedEnemy() ?? 0) : "対面を検索"}
                  onFocus={() => setEnemyMenuOpen(true)}
                  onInput={(event) => {
                    setEnemyQuery(event.currentTarget.value);
                    setEnemyMenuOpen(true);
                  }}
                />
                <Show when={enemyMenuOpen()}>
                  <div class="desktop-combobox-menu">
                    <button type="button" onClick={() => chooseEnemy(0)}>
                      指定なし（Best Build）
                    </button>
                    <For each={matchingEnemies()}>
                      {(item) => (
                        <button type="button" onClick={() => chooseEnemy(item.key)}>
                          <Icon url={champIconByKey(item.key)} />
                          {item.name}
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </section>
            <section class="desktop-card desktop-counter-card">
              <div class="desktop-section-heading">
                <h2>カウンター</h2>
                <span>{roleLabel(selectedRole())}</span>
              </div>
              <Counters championId={championId()} role={selectedRole()} />
            </section>
          </aside>
          <section class="desktop-card desktop-build-card">
            <div class="desktop-section-heading">
              <h2>
                {selectedEnemy() ? `VS ${champName(selectedEnemy() ?? 0)}` : "おすすめルーン"}
              </h2>
              <span>{selectedEnemy() ? "MATCHUP BUILD" : "BEST BUILD"}</span>
            </div>
            <BuildArea championId={championId()} role={selectedRole()} enemyId={selectedEnemy()} />
            <StatsRow championId={championId()} role={selectedRole()} enemyId={selectedEnemy()} />
            <Show when={detailsValue()}>
              {(value) => (
                <div class="desktop-build-extras">
                  <SkillOrder
                    order={value().skillOrder}
                    championImageId={champion()?.imageId ?? ""}
                  />
                  <Show when={value().items.length > 0}>
                    <div class="desktop-item-build">
                      <span>ITEM BUILD</span>
                      <For each={value().items}>
                        {(item) => (
                          <Icon
                            url={itemIconUrl(item.itemId)}
                            title={`${item.name} · ${item.reason}`}
                          />
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              )}
            </Show>
            <Show when={details().state === "loading"}>
              <div class="desktop-build-details-status">アイテムとスキル順を読み込んでいます…</div>
            </Show>
            <Show when={details().state === "err"}>
              <div class="desktop-build-details-status is-error">
                <span>このデータソースではアイテムまたはスキル順を取得できませんでした。</span>
                <button type="button" onClick={() => buildDetailsCache.refetch(detailsKey())}>
                  再試行
                </button>
              </div>
            </Show>
            <ImportButton
              championId={championId()}
              role={selectedRole()}
              enemyId={selectedEnemy()}
            />
          </section>
        </div>
      </div>
    </Show>
  );
}

export function LivePage() {
  return (
    <ScrollArea class="h-full" contentClass="desktop-page">
      <PageHeader
        eyebrow="LIVE"
        title="現在のゲーム"
        description="進行中の試合情報とおすすめを表示します。"
      />
      <Show
        when={phase()?.inGame}
        fallback={
          <section class="desktop-card desktop-empty desktop-live-empty">
            現在進行中のゲームはありません。
          </section>
        }
      >
        <section class="desktop-live-panel">
          <InGamePanel embedded />
        </section>
      </Show>
    </ScrollArea>
  );
}

export function SettingsPage() {
  return (
    <ScrollArea class="h-full" contentClass="desktop-page desktop-settings-page">
      <PageHeader
        eyebrow="PREFERENCES"
        title="設定"
        description="オーバーレイ、インポート、表示方法を管理します。"
      />
      <section class="desktop-card">
        <SettingsForm />
      </section>
      <Show when={developerMode()}>
        <section class="desktop-card">
          <DebugPanel />
        </section>
      </Show>
    </ScrollArea>
  );
}

export function NotFoundPage() {
  return (
    <div class="desktop-page">
      <section class="desktop-card desktop-empty">
        ページが見つかりません。<A href="/">ホームへ戻る</A>
      </section>
    </div>
  );
}
