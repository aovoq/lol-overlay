import { invoke } from "@tauri-apps/api/core";
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { champIconByKey, champName, profileIconUrl } from "../../assets";
import {
  addPlayerHistory,
  filterChampionStats,
  filterMatches,
  loadPlayerHistory,
  parseRiotId,
  savePlayerHistory,
  summarizeMatches,
} from "../../lib/playerSearch";
import { createPlayerStatsState } from "../../state/playerStats";
import type { PlayerRef } from "../../types";
import { Icon } from "../Icon";
import { ScrollArea } from "../ScrollArea";

const REGIONS = [
  "KR",
  "JP1",
  "NA1",
  "EUW1",
  "EUN1",
  "OC1",
  "BR1",
  "LA1",
  "LA2",
  "TR1",
  "RU",
  "PH2",
  "SG2",
  "TH2",
  "TW2",
  "VN2",
];

function rankLabel(tier?: string | null, division?: string | null) {
  if (!tier) return "Unranked";
  return `${tier} ${division ?? ""}`.trim();
}

function number(value?: number | null, suffix = "") {
  return value === undefined || value === null ? "—" : `${value.toLocaleString()}${suffix}`;
}

function formatDate(timestamp: number) {
  if (!timestamp) return "—";
  const milliseconds = timestamp < 10_000_000_000 ? timestamp * 1_000 : timestamp;
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(
    milliseconds,
  );
}

export function SummonerPage() {
  const state = createPlayerStatsState();
  const [region, setRegion] = createSignal("JP1");
  const [riotId, setRiotId] = createSignal("");
  const [inputError, setInputError] = createSignal("");
  const [history, setHistory] = createSignal<PlayerRef[]>(loadPlayerHistory(window.localStorage));
  const [queue, setQueue] = createSignal<number>();
  const [championFilter, setChampionFilter] = createSignal<number>();
  const [roleFilter, setRoleFilter] = createSignal("");
  const [statsQueueFilter, setStatsQueueFilter] = createSignal("");

  const visibleMatches = createMemo(() => filterMatches(state.matches()?.matches ?? [], queue()));
  const summary = createMemo(() => summarizeMatches(visibleMatches()));
  const visibleStats = createMemo(() =>
    filterChampionStats(state.championStats(), {
      championId: championFilter(),
      role: roleFilter(),
      queue: statsQueueFilter(),
    }),
  );
  const championOptions = createMemo(() =>
    [...new Set(state.championStats().map((entry) => entry.championId))].sort((a, b) =>
      (champName(a) || String(a)).localeCompare(champName(b) || String(b)),
    ),
  );

  function remember(player: PlayerRef) {
    const next = addPlayerHistory(history(), player);
    setHistory(next);
    savePlayerHistory(window.localStorage, next);
  }

  async function search(player?: PlayerRef) {
    try {
      const target = player ?? parseRiotId(region(), riotId());
      setInputError("");
      setRegion(target.platformId);
      setRiotId(`${target.gameName}#${target.tagLine}`);
      remember(target);
      await state.search(target);
    } catch (error) {
      setInputError(error instanceof Error ? error.message : String(error));
    }
  }

  function removeHistory(player: PlayerRef) {
    const next = history().filter(
      (entry) =>
        entry.platformId !== player.platformId ||
        entry.gameName !== player.gameName ||
        entry.tagLine !== player.tagLine,
    );
    setHistory(next);
    savePlayerHistory(window.localStorage, next);
  }

  onMount(async () => {
    await state.initialize().catch(() => undefined);
    const params = new URLSearchParams(window.location.search);
    const mockCurrent = params.has("player-stats-mock") ? params.get("current-player") : null;
    const current = mockCurrent
      ? parseRiotId("JP1", mockCurrent)
      : await invoke<PlayerRef | null>("get_current_player_ref").catch(() => null);
    if (current) await search(current);
    else if (history()[0]) await search(history()[0]);
  });

  return (
    <ScrollArea class="h-full" contentClass="desktop-page summoner-page">
      <header class="desktop-page-header summoner-heading">
        <div>
          <span class="desktop-eyebrow">PLAYER STATS</span>
          <h1>サモナー</h1>
          <p>選択したデータ提供元だけを使い、プロフィールと戦績を表示します。</p>
        </div>
        <label class="summoner-provider-field">
          <span>Provider</span>
          <select
            value={state.source()}
            disabled={state.status() === "loading"}
            onChange={(event) => state.selectSource(event.currentTarget.value)}
          >
            <For each={state.sources()}>
              {(provider) => <option value={provider.id}>{provider.label}</option>}
            </For>
          </select>
        </label>
      </header>

      <form
        class="desktop-card summoner-search"
        onSubmit={(event) => {
          event.preventDefault();
          void search();
        }}
      >
        <label>
          <span>Region</span>
          <select value={region()} onChange={(event) => setRegion(event.currentTarget.value)}>
            <For each={REGIONS}>{(value) => <option value={value}>{value}</option>}</For>
          </select>
        </label>
        <label class="summoner-riot-id">
          <span>Riot ID</span>
          <input
            value={riotId()}
            onInput={(event) => setRiotId(event.currentTarget.value)}
            placeholder="GameName#Tag"
            autocomplete="off"
            aria-describedby="summoner-search-error"
          />
        </label>
        <button type="submit" disabled={state.status() === "loading"}>
          {state.status() === "loading" ? "検索中…" : "検索"}
        </button>
        <Show when={inputError()}>
          <p id="summoner-search-error" class="summoner-inline-error" role="alert">
            {inputError()}
          </p>
        </Show>
      </form>

      <Show when={history().length > 0}>
        <section class="summoner-history" aria-label="検索履歴">
          <div class="desktop-section-heading">
            <h2>検索履歴</h2>
            <button
              type="button"
              onClick={() => {
                setHistory([]);
                savePlayerHistory(window.localStorage, []);
              }}
            >
              すべて削除
            </button>
          </div>
          <div class="summoner-history-list">
            <For each={history()}>
              {(entry) => (
                <div class="summoner-history-chip">
                  <button type="button" onClick={() => void search(entry)}>
                    <small>{entry.platformId}</small>
                    {entry.gameName}#{entry.tagLine}
                  </button>
                  <button
                    type="button"
                    class="summoner-history-remove"
                    aria-label={`${entry.gameName}の履歴を削除`}
                    onClick={() => removeHistory(entry)}
                  >
                    ×
                  </button>
                </div>
              )}
            </For>
          </div>
        </section>
      </Show>

      <Show when={state.status() === "loading"}>
        <section class="desktop-card summoner-skeleton" aria-label="サモナー情報を読み込み中">
          <i />
          <div>
            <i />
            <i />
          </div>
        </section>
      </Show>

      <Show when={state.status() === "error"}>
        <section class="desktop-card summoner-state" role="alert">
          <strong>
            {state.error()?.kind === "notFound"
              ? "サモナーが見つかりません"
              : state.error()?.kind === "validation"
                ? "入力またはAPI契約を確認してください"
                : state.error()?.kind === "rateLimited"
                  ? "しばらく待ってから再試行してください"
                  : "データを取得できませんでした"}
          </strong>
          <p>
            {state.error()?.kind === "rateLimited" && state.error()?.retryAfter
              ? `再試行まで約 ${state.error()?.retryAfter} 秒です。`
              : state.error()?.message}
          </p>
          <button
            type="button"
            onClick={() => {
              const current = state.player();
              if (current) void state.search(current);
            }}
          >
            再試行
          </button>
        </section>
      </Show>

      <Show when={state.profile()}>
        {(profile) => (
          <>
            <section class="desktop-card summoner-profile">
              <Icon
                url={
                  profile().profileIconId == null
                    ? ""
                    : profileIconUrl(profile().profileIconId ?? 0)
                }
                class="summoner-profile-icon"
              />
              <div class="summoner-profile-identity">
                <span>{profile().identity.platformId}</span>
                <h2>
                  {profile().identity.gameName}
                  <small>#{profile().identity.tagLine}</small>
                </h2>
                <p>Level {number(profile().level)}</p>
              </div>
              <div class="summoner-profile-actions">
                <span>Fetched {formatDate(profile().fetchedAt)}</span>
                <button type="button" onClick={() => void state.refresh()}>
                  再読み込み
                </button>
              </div>
              <div class="summoner-rank-grid">
                <For
                  each={["RANKED_SOLO_5x5", "RANKED_FLEX_SR"]}
                  fallback={<div class="summoner-rank-card">ランク情報はありません。</div>}
                >
                  {(queueName) => {
                    const rank = () => profile().ranks.find((entry) => entry.queue === queueName);
                    return (
                      <article class="summoner-rank-card">
                        <span>{queueName === "RANKED_SOLO_5x5" ? "Solo / Duo" : "Flex"}</span>
                        <strong>{rankLabel(rank()?.tier, rank()?.division)}</strong>
                        <p>{number(rank()?.lp, " LP")}</p>
                        <small>
                          {number(rank()?.wins)}W · {number(rank()?.losses)}L
                        </small>
                      </article>
                    );
                  }}
                </For>
                <article class="summoner-rank-card">
                  <span>Ladder</span>
                  <strong>{number(profile().ladderRank, "位")}</strong>
                  <p>{number(profile().ladderPercentile, "%")}</p>
                </article>
              </div>
              <Show when={profile().previousSeasons.length > 0}>
                <div class="summoner-season-list">
                  <For each={profile().previousSeasons}>
                    {(season) => (
                      <span>
                        S{season.season} · {rankLabel(season.tier, season.division)}
                      </span>
                    )}
                  </For>
                </div>
              </Show>
            </section>

            <section class="summoner-section">
              <div class="desktop-section-heading summoner-section-heading">
                <div class="summoner-section-title">
                  <h2>最近の試合</h2>
                  <span class="summoner-record">
                    {summary().games} games · {summary().wins}W {summary().losses}L ·{" "}
                    {summary().winRate === undefined
                      ? "—"
                      : `${Math.round((summary().winRate ?? 0) * 100)}%`}
                  </span>
                </div>
                <select
                  aria-label="キューで試合を絞り込む"
                  value={queue() ?? ""}
                  onChange={(event) => {
                    const value = event.currentTarget.value;
                    const next = value ? Number(value) : undefined;
                    setQueue(next);
                    void state.setQueueFilter(next);
                  }}
                >
                  <option value="">すべてのキュー</option>
                  <option value="420">Solo / Duo</option>
                  <option value="440">Flex</option>
                  <option value="450">ARAM</option>
                  <option value="1700">Arena</option>
                </select>
              </div>
              <div class="summoner-match-list">
                <For
                  each={visibleMatches()}
                  fallback={
                    <div class="desktop-card summoner-state">一致する試合はありません。</div>
                  }
                >
                  {(match) => (
                    <details
                      class={`desktop-card summoner-match ${match.win ? "is-win" : "is-loss"}`}
                    >
                      <summary>
                        <Icon url={champIconByKey(match.championId)} class="summoner-match-icon" />
                        <div>
                          <strong>{champName(match.championId) || `#${match.championId}`}</strong>
                          <span>{match.remake ? "Remake" : match.win ? "勝利" : "敗北"}</span>
                        </div>
                        <span class="summoner-match-kda">
                          {match.kills} / {match.deaths} / {match.assists}
                        </span>
                        <span>{match.role ?? "—"}</span>
                        <time>{formatDate(match.startedAt)}</time>
                      </summary>
                      <div class="summoner-participants">
                        <For each={match.participants}>
                          {(participant) => (
                            <div>
                              <Icon url={champIconByKey(participant.championId)} />
                              <span
                                title={`${participant.gameName ?? "Unknown"}#${participant.tagLine ?? ""}`}
                              >
                                {participant.gameName ?? "Unknown"}
                              </span>
                              <b>
                                {participant.kills}/{participant.deaths}/{participant.assists}
                              </b>
                            </div>
                          )}
                        </For>
                      </div>
                    </details>
                  )}
                </For>
              </div>
              <Show when={(state.matches()?.partialFailures.length ?? 0) > 0}>
                <div class="summoner-partial" role="status">
                  {state.matches()?.partialFailures.length ?? 0}件の詳細取得に失敗しました。
                  <Show
                    when={state.matches()?.partialFailures.some((failure) => failure.retryable)}
                  >
                    <button type="button" onClick={() => void state.retryPartialFailures()}>
                      失敗分を再試行
                    </button>
                  </Show>
                </div>
              </Show>
              <Show when={state.matches()?.nextCursor}>
                <button
                  type="button"
                  class="summoner-load-more"
                  disabled={state.loadingMore()}
                  onClick={() => void state.loadMore()}
                >
                  {state.loadingMore() ? "読み込み中…" : "さらに20件読み込む"}
                </button>
              </Show>
            </section>

            <section class="summoner-section">
              <div class="desktop-section-heading summoner-section-heading">
                <div class="summoner-section-title">
                  <h2>Champion Stats</h2>
                  <span>{visibleStats().length} champions</span>
                </div>
                <div class="summoner-stat-filters">
                  <select
                    aria-label="チャンピオンで絞り込む"
                    value={championFilter() ?? ""}
                    onChange={(event) =>
                      setChampionFilter(
                        event.currentTarget.value ? Number(event.currentTarget.value) : undefined,
                      )
                    }
                  >
                    <option value="">すべてのチャンピオン</option>
                    <For each={championOptions()}>
                      {(id) => <option value={id}>{champName(id) || `#${id}`}</option>}
                    </For>
                  </select>
                  <select
                    aria-label="ロールで絞り込む"
                    value={roleFilter()}
                    onChange={(event) => setRoleFilter(event.currentTarget.value)}
                  >
                    <option value="">すべてのロール</option>
                    <For each={["Top", "Jungle", "Middle", "Bottom", "Supporter"]}>
                      {(value) => <option value={value}>{value}</option>}
                    </For>
                  </select>
                  <select
                    aria-label="Champion Statsのキューで絞り込む"
                    value={statsQueueFilter()}
                    onChange={(event) => setStatsQueueFilter(event.currentTarget.value)}
                  >
                    <option value="">すべてのキュー</option>
                    <option value="RANKED_SOLO_5x5">Solo / Duo</option>
                    <option value="RANKED_FLEX_SR">Flex</option>
                  </select>
                </div>
              </div>
              <div class="desktop-card summoner-stats-table-wrap">
                <table class="summoner-stats-table">
                  <thead>
                    <tr>
                      <th>Champion</th>
                      <th>Role</th>
                      <th>Games</th>
                      <th>Win rate</th>
                      <th>KDA</th>
                      <th>CS / min</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={visibleStats()}>
                      {(entry) => (
                        <tr>
                          <td>
                            <Icon url={champIconByKey(entry.championId)} />
                            <strong>{champName(entry.championId) || `#${entry.championId}`}</strong>
                          </td>
                          <td>{entry.role ?? "—"}</td>
                          <td>{number(entry.games)}</td>
                          <td>
                            {Number.isFinite(entry.winRate)
                              ? `${Math.round(entry.winRate * 100)}%`
                              : "—"}
                          </td>
                          <td>{entry.kda == null ? "—" : entry.kda.toFixed(2)}</td>
                          <td>{entry.csPerMinute == null ? "—" : entry.csPerMinute.toFixed(1)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
                <Show when={visibleStats().length === 0}>
                  <div class="summoner-state">一致するChampion Statsはありません。</div>
                </Show>
              </div>
            </section>
          </>
        )}
      </Show>

      <Show when={state.status() === "idle" && history().length === 0}>
        <section class="desktop-card summoner-state">
          <strong>Riot IDを検索してください</strong>
          <p>GameName#Tagの形式で入力すると、選択中Providerから取得します。</p>
        </section>
      </Show>
    </ScrollArea>
  );
}
