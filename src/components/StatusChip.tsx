import { createMemo, For, Show } from "solid-js";
import { assetsReady, champIconByKey, champName, profileIconUrl } from "../assets";
import { APP_NAME, fmtTier } from "../lib/openlol";
import { matchHistory, phase, summoner } from "../state/backend";
import type { RecentGame } from "../types";
import { Icon } from "./Icon";

function formSummary(games: RecentGame[]) {
  const wins = games.filter((g) => g.win).length;
  const losses = games.length - wins;
  const kills = games.reduce((n, g) => n + g.kills, 0);
  const deaths = games.reduce((n, g) => n + g.deaths, 0);
  const assists = games.reduce((n, g) => n + g.assists, 0);
  const kda = deaths > 0 ? ((kills + assists) / deaths).toFixed(2) : "Perfect";

  let streak = 1;
  while (streak < games.length && games[streak].win === games[0].win) streak++;
  const streakLabel = streak >= 2 ? `${streak}${games[0].win ? "連勝" : "連敗"}` : "";

  return {
    record: `${wins}W ${losses}L`,
    kda: `KDA ${kda}`,
    streakLabel,
    streakWin: games[0].win && streak >= 3,
    streakLoss: !games[0].win && streak >= 3,
  };
}

/** Control-window home: brand bar + connection state + rank/form dashboard.
 * The one glance between games that answers "am I connected, how am I doing". */
export function StatusChip() {
  const p = () => phase();
  const s = () => summoner();
  const games = () => matchHistory();
  const summary = createMemo(() => {
    const g = games();
    return g && g.length > 0 ? formSummary(g) : null;
  });

  const dotClass = () => {
    const ph = p();
    if (!ph) return "bg-hx-red";
    if (ph.inGame) return "bg-hx-up";
    if (ph.clientUp) return "bg-hx-accent";
    return "bg-hx-red";
  };

  const statusText = () => {
    const ph = p();
    if (!ph?.clientUp) return "WAITING FOR CLIENT";
    if (ph.inGame) return `IN GAME · ${ph.phase.toUpperCase()}`;
    return ph.phase.toUpperCase();
  };

  const rank = createMemo(() => {
    const e = s();
    if (!e) return null;
    if (!e.soloTier) return { tier: "UNRANKED", lp: "", winRate: "", record: "" };
    const division = e.soloDivision && e.soloDivision !== "NA" ? ` ${e.soloDivision}` : "";
    const gameCount = e.soloWins + e.soloLosses;
    return {
      tier: `${fmtTier(e.soloTier)}${division}`.toUpperCase(),
      lp: `${e.soloLp} LP`,
      winRate: gameCount > 0 ? `${Math.round((e.soloWins / gameCount) * 100)}% WR` : "",
      record: gameCount > 0 ? `${e.soloWins}W ${e.soloLosses}L` : "",
    };
  });

  return (
    <div class="flex flex-col gap-3">
      {/* Brand + connection state */}
      <div class="flex items-center justify-between">
        <span class="inline-flex items-center gap-2 text-hx-accent font-hx-display font-extrabold text-[12px] tracking-[0.3em]">
          <svg
            viewBox="0 0 24 24"
            aria-hidden="true"
            class="w-[13px] h-[13px] fill-none stroke-current stroke-[1.6]"
          >
            <polygon points="12 2.5 20.2 7.25 20.2 16.75 12 21.5 3.8 16.75 3.8 7.25" />
          </svg>
          {APP_NAME}
        </span>
        <span class="inline-flex items-center gap-1.5 border border-hx-border rounded-[3px] px-2 py-[3px] text-[9px] font-bold tracking-[0.14em] text-hx-muted whitespace-nowrap">
          <span class={`w-[7px] h-[7px] rounded-full ${dotClass()}`} />
          {statusText()}
        </span>
      </div>

      {/* Summoner profile: rank is the hero number */}
      <Show
        when={s()}
        fallback={
          <div class="text-[11px] text-hx-muted py-1">League クライアントの起動を待っています…</div>
        }
      >
        {(e) => (
          <div class="flex items-center gap-3">
            <Show when={assetsReady()}>
              <Icon
                url={profileIconUrl(e().profileIconId)}
                class="w-11 h-11 rounded border border-hx-border"
              />
            </Show>
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[12px] font-bold text-hx-text truncate">
                {e().tagLine ? `${e().gameName} #${e().tagLine}` : e().gameName}
              </span>
              <Show when={rank()}>
                {(r) => (
                  <span class="flex items-baseline gap-1.5 whitespace-nowrap">
                    <span class="font-hx-display font-extrabold text-[15px] tracking-[0.06em] text-hx-text-strong">
                      {r().tier}
                    </span>
                    <Show when={r().lp}>
                      <span class="font-bold text-[12px] text-hx-accent">{r().lp}</span>
                    </Show>
                    <Show when={r().winRate}>
                      <span class="text-[11px] text-hx-muted">
                        {r().winRate} · {r().record}
                      </span>
                    </Show>
                  </span>
                )}
              </Show>
            </div>
          </div>
        )}
      </Show>

      {/* Recent form */}
      <Show when={games()?.length}>
        <div class="flex items-center gap-2.5 pt-2.5 border-t border-hx-border">
          <div class="flex gap-1">
            <Show when={assetsReady()}>
              <For each={games()}>
                {(g) => {
                  const name = champName(g.championId);
                  const line = `${g.kills}/${g.deaths}/${g.assists} · ${g.win ? "勝利" : "敗北"}`;
                  return (
                    <Icon
                      url={champIconByKey(g.championId)}
                      class={`w-[22px] h-[22px] rounded-[3px] border-b-2 ${
                        g.win
                          ? "border-b-hx-up opacity-95"
                          : "border-b-hx-red grayscale-50 opacity-75"
                      }`}
                      title={name ? `${name} · ${line}` : line}
                    />
                  );
                }}
              </For>
            </Show>
          </div>
          <Show when={summary()}>
            {(sm) => (
              <span class="flex items-center gap-1.5 text-[11px] text-hx-muted whitespace-nowrap">
                {sm().record} · {sm().kda}
                <Show when={sm().streakLabel}>
                  <span
                    class={`px-1.5 py-px rounded-[3px] border text-[10px] font-bold ${
                      sm().streakWin
                        ? "text-hx-up border-hx-up/50"
                        : sm().streakLoss
                          ? "text-hx-red border-hx-red-soft"
                          : "text-hx-muted border-hx-border"
                    }`}
                  >
                    {sm().streakLabel}
                  </span>
                </Show>
              </span>
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
}
