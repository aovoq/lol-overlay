import type { PlayerChampionStats, PlayerMatch, PlayerRef } from "../types";

export const PLAYER_HISTORY_KEY = "lol-overlay.player-search-history.v1";
export const PLAYER_HISTORY_LIMIT = 10;

export function parseRiotId(platformId: string, input: string): PlayerRef {
  const trimmed = input.trim();
  const separator = trimmed.lastIndexOf("#");
  if (separator <= 0 || separator === trimmed.length - 1) {
    throw new Error("Enter a Riot ID as GameName#Tag");
  }
  const gameName = trimmed.slice(0, separator);
  const tagLine = trimmed.slice(separator + 1);
  if (!gameName.trim() || !tagLine.trim() || tagLine.includes("#")) {
    throw new Error("Enter a Riot ID as GameName#Tag");
  }
  return { platformId: platformId.toUpperCase(), gameName, tagLine };
}

function isPlayerRef(value: unknown): value is PlayerRef {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<PlayerRef>;
  return (
    typeof candidate.platformId === "string" &&
    typeof candidate.gameName === "string" &&
    typeof candidate.tagLine === "string" &&
    candidate.platformId.length > 0 &&
    candidate.gameName.length > 0 &&
    candidate.tagLine.length > 0
  );
}

export function playerKey(player: PlayerRef): string {
  return `${player.platformId}\u0000${player.gameName}\u0000${player.tagLine}`;
}

export function addPlayerHistory(history: PlayerRef[], player: PlayerRef): PlayerRef[] {
  return [player, ...history.filter((entry) => playerKey(entry) !== playerKey(player))].slice(
    0,
    PLAYER_HISTORY_LIMIT,
  );
}

export function loadPlayerHistory(storage: Pick<Storage, "getItem" | "removeItem">): PlayerRef[] {
  try {
    const raw = storage.getItem(PLAYER_HISTORY_KEY);
    if (!raw) return [];
    const value: unknown = JSON.parse(raw);
    if (!Array.isArray(value)) throw new Error("history is not an array");
    return value.filter(isPlayerRef).slice(0, PLAYER_HISTORY_LIMIT);
  } catch {
    storage.removeItem(PLAYER_HISTORY_KEY);
    return [];
  }
}

export function savePlayerHistory(storage: Pick<Storage, "setItem">, history: PlayerRef[]): void {
  storage.setItem(PLAYER_HISTORY_KEY, JSON.stringify(history.slice(0, PLAYER_HISTORY_LIMIT)));
}

export function filterMatches(matches: PlayerMatch[], queueId?: number): PlayerMatch[] {
  return queueId === undefined ? matches : matches.filter((match) => match.queueId === queueId);
}

export interface MatchSummary {
  games: number;
  wins: number;
  losses: number;
  winRate?: number;
}

export function summarizeMatches(matches: PlayerMatch[]): MatchSummary {
  const counted = matches.filter((match) => !match.remake);
  const wins = counted.filter((match) => match.win).length;
  return {
    games: counted.length,
    wins,
    losses: counted.length - wins,
    winRate: counted.length > 0 ? wins / counted.length : undefined,
  };
}

export function filterChampionStats(
  stats: PlayerChampionStats[],
  filters: { championId?: number; role?: string; queue?: string },
): PlayerChampionStats[] {
  return stats.filter(
    (entry) =>
      (filters.championId === undefined || entry.championId === filters.championId) &&
      (!filters.role || entry.role === filters.role) &&
      (!filters.queue || entry.queue === filters.queue),
  );
}
