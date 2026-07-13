import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import type {
  MatchPage,
  PlayerChampionStats,
  PlayerProfile,
  PlayerProviderDescriptor,
  PlayerRef,
  RefreshResult,
} from "../types";

export interface PlayerStatsGateway {
  listSources(): Promise<PlayerProviderDescriptor[]>;
  getSource(): Promise<string>;
  setSource(source: string): Promise<void>;
  profile(player: PlayerRef, forceRefresh: boolean): Promise<PlayerProfile>;
  matches(
    player: PlayerRef,
    cursor?: string,
    queue?: number,
    forceRefresh?: boolean,
  ): Promise<MatchPage>;
  championStats(
    player: PlayerRef,
    filters: { season?: string; queue?: string; role?: string },
    forceRefresh: boolean,
  ): Promise<PlayerChampionStats[]>;
  refresh(player: PlayerRef): Promise<RefreshResult>;
}

export const tauriPlayerStatsGateway: PlayerStatsGateway = {
  listSources: () => invoke("list_player_stats_sources"),
  getSource: () => invoke("get_player_stats_source"),
  setSource: (source) => invoke("set_player_stats_source", { source }),
  profile: (player, forceRefresh) => invoke("get_player_profile", { player, forceRefresh }),
  matches: (player, cursor, queue, forceRefresh = false) =>
    invoke("get_player_matches", { player, cursor, queue, forceRefresh }),
  championStats: (player, filters, forceRefresh) =>
    invoke("get_player_champion_stats", { player, ...filters, forceRefresh }),
  refresh: (player) => invoke("refresh_player_data", { player }),
};

export type PlayerViewStatus = "idle" | "loading" | "ready" | "empty" | "error" | "partial";

export interface PlayerViewError {
  kind: "notFound" | "validation" | "rateLimited" | "unknown";
  message: string;
  retryAfter?: number;
}

function classifyError(error: unknown): PlayerViewError {
  const message = error instanceof Error ? error.message : String(error);
  if (/player-http:404|\b404\b/i.test(message)) return { kind: "notFound", message };
  if (/player-http:422|\b422\b/i.test(message)) return { kind: "validation", message };
  if (/player-http:429|\b429\b/i.test(message)) {
    const retry = message.match(/retry-after=([0-9]+)/i)?.[1];
    return { kind: "rateLimited", message, retryAfter: retry ? Number(retry) : undefined };
  }
  return { kind: "unknown", message };
}

export function createPlayerStatsState(gateway: PlayerStatsGateway = tauriPlayerStatsGateway) {
  const [status, setStatus] = createSignal<PlayerViewStatus>("idle");
  const [sources, setSources] = createSignal<PlayerProviderDescriptor[]>([]);
  const [source, setSourceState] = createSignal("deeplol");
  const [player, setPlayer] = createSignal<PlayerRef>();
  const [profile, setProfile] = createSignal<PlayerProfile>();
  const [matches, setMatches] = createSignal<MatchPage>();
  const [championStats, setChampionStats] = createSignal<PlayerChampionStats[]>([]);
  const [error, setError] = createSignal<PlayerViewError>();
  const [loadingMore, setLoadingMore] = createSignal(false);
  let generation = 0;
  let queueFilter: number | undefined;
  let championFilters: { season?: string; queue?: string; role?: string } = {};

  async function initialize() {
    const [available, active] = await Promise.all([gateway.listSources(), gateway.getSource()]);
    setSources(available.filter((entry) => entry.capabilities.playerProfile));
    setSourceState(active);
  }

  async function search(nextPlayer: PlayerRef, forceRefresh = false) {
    const request = ++generation;
    setPlayer(nextPlayer);
    setStatus("loading");
    setError(undefined);
    setMatches(undefined);
    setChampionStats([]);
    try {
      const [nextProfile, nextMatches, nextChampions] = await Promise.all([
        gateway.profile(nextPlayer, forceRefresh),
        gateway.matches(nextPlayer, undefined, queueFilter, forceRefresh),
        gateway.championStats(nextPlayer, championFilters, forceRefresh),
      ]);
      if (request !== generation) return;
      setProfile(nextProfile);
      setMatches(nextMatches);
      setChampionStats(nextChampions);
      if (nextMatches.partialFailures.length > 0) setStatus("partial");
      else if (nextMatches.matches.length === 0 && nextChampions.length === 0) setStatus("empty");
      else setStatus("ready");
    } catch (cause) {
      if (request !== generation) return;
      setError(classifyError(cause));
      setStatus("error");
    }
  }

  async function selectSource(nextSource: string) {
    if (nextSource === source()) return;
    const descriptor = sources().find((entry) => entry.id === nextSource);
    if (!descriptor?.capabilities.playerProfile) throw new Error("Unsupported player provider");
    ++generation;
    await gateway.setSource(nextSource);
    setSourceState(nextSource);
    const current = player();
    if (current) await search(current);
  }

  async function loadMore() {
    const current = player();
    const page = matches();
    if (!current || !page?.nextCursor || loadingMore()) return;
    const request = generation;
    setLoadingMore(true);
    try {
      const next = await gateway.matches(current, page.nextCursor, queueFilter, false);
      if (request !== generation) return;
      setMatches({
        ...next,
        matches: [...page.matches, ...next.matches],
        partialFailures: [...page.partialFailures, ...next.partialFailures],
      });
      setStatus(next.partialFailures.length > 0 ? "partial" : "ready");
    } catch (cause) {
      if (request === generation) setError(classifyError(cause));
    } finally {
      if (request === generation) setLoadingMore(false);
    }
  }

  async function retryPartialFailures() {
    const current = player();
    if (!current || matches()?.partialFailures.length === 0) return;
    await search(current, true);
  }

  async function refresh() {
    const current = player();
    if (!current) return;
    await gateway.refresh(current);
    await search(current, true);
  }

  async function setQueueFilter(queue?: number) {
    queueFilter = queue;
    const current = player();
    if (current) await search(current);
  }

  async function setChampionFilters(filters: typeof championFilters) {
    championFilters = filters;
    const current = player();
    if (current) await search(current);
  }

  return {
    status,
    sources,
    source,
    player,
    profile,
    matches,
    championStats,
    error,
    loadingMore,
    initialize,
    search,
    selectSource,
    loadMore,
    retryPartialFailures,
    refresh,
    setQueueFilter,
    setChampionFilters,
  };
}
