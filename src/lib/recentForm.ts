import type { RecentGame } from "../types";

export interface RecentFormSummary {
  record: string;
  kda: string;
  streakLabel: string;
  streakWin: boolean;
  streakLoss: boolean;
}

/** Aggregate recent games (newest first) into a one-line form summary. */
export function formSummary(games: RecentGame[]): RecentFormSummary | null {
  if (games.length === 0) return null;

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
