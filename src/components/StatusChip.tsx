import { createMemo, For, Show } from "solid-js";
import { assetsReady, champIconByKey, champName, profileIconUrl } from "../assets";
import { fmtTier } from "../lib/hexgate";
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
  const streakLabel =
    streak >= 2 ? ` · ${streak}${games[0].win ? "連勝" : "連敗"}` : "";

  return {
    text: `${wins}W ${losses}L · KDA ${kda}${streakLabel}`,
    streakWin: games[0].win && streak >= 3,
    streakLoss: !games[0].win && streak >= 3,
  };
}

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
    if (!ph) return "bg-hx-red shadow-[0_0_8px_currentColor]";
    if (ph.inGame) return "bg-hx-up shadow-[0_0_8px_currentColor]";
    if (ph.clientUp) return "bg-hx-gold shadow-[0_0_8px_currentColor]";
    return "bg-hx-red shadow-[0_0_8px_currentColor]";
  };

  const statusText = () => {
    const ph = p();
    if (!ph) return "Waiting for League client…";
    if (ph.inGame) return `In game (${ph.phase})`;
    if (ph.clientUp) return `Client: ${ph.phase}`;
    return "Waiting for League client…";
  };

  const rankText = () => {
    const e = s();
    if (!e) return "";
    if (e.soloTier) {
      const division =
        e.soloDivision && e.soloDivision !== "NA" ? ` ${e.soloDivision}` : "";
      const gameCount = e.soloWins + e.soloLosses;
      const winRate =
        gameCount > 0
          ? ` · ${e.soloWins}W ${e.soloLosses}L (${Math.round((e.soloWins / gameCount) * 100)}%)`
          : "";
      return `${fmtTier(e.soloTier)}${division} ${e.soloLp} LP${winRate}`;
    }
    return "Unranked";
  };

  return (
    <div class="panel fixed left-4 bottom-4 flex items-center gap-2 text-hx-muted text-xs tracking-wide">
      <span class={`w-[9px] h-[9px] rounded-full ${dotClass()}`} />
      <span>{statusText()}</span>
      <Show when={s()}>
        {(e) => (
          <div class="flex items-center gap-2 ml-1.5 pl-2.5 border-l border-hx-border">
            <Show when={assetsReady()}>
              <Icon
                url={profileIconUrl(e().profileIconId)}
                class="w-[26px] h-[26px] rounded-full border border-hx-border"
              />
            </Show>
            <div class="flex flex-col leading-tight">
              <span class="text-hx-text-strong font-semibold">
                {e().tagLine
                  ? `${e().gameName} #${e().tagLine}`
                  : e().gameName}
              </span>
              <span class="text-hx-muted text-[11px]">{rankText()}</span>
              <Show when={games() && games()!.length > 0}>
                <div class="flex flex-col gap-0.5 mt-1">
                  <div class="flex gap-0.5">
                    <Show when={assetsReady()}>
                      <For each={games()}>
                        {(g) => {
                          const name = champName(g.championId);
                          const title = name
                            ? `${name} · ${g.kills}/${g.deaths}/${g.assists} · ${g.win ? "勝利" : "敗北"}`
                            : `${g.kills}/${g.deaths}/${g.assists} · ${g.win ? "勝利" : "敗北"}`;
                          return (
                            <Icon
                              url={champIconByKey(g.championId)}
                              class={`w-[18px] h-[18px] rounded-[3px] border-b-2 ${
                                g.win
                                  ? "border-b-hx-up opacity-95"
                                  : "border-b-hx-red grayscale-50 opacity-75"
                              }`}
                              title={title}
                            />
                          );
                        }}
                      </For>
                    </Show>
                  </div>
                  <Show when={summary()}>
                    {(sm) => (
                      <span
                        class={`text-[11px] ${
                          sm().streakWin
                            ? "text-hx-up"
                            : sm().streakLoss
                              ? "text-hx-red"
                              : "text-hx-muted"
                        }`}
                      >
                        {sm().text}
                      </span>
                    )}
                  </Show>
                </div>
              </Show>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
