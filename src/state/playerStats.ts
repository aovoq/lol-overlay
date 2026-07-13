import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import type {
  MatchPage,
  PlayerChampionStats,
  PlayerProfile,
  PlayerProviderDescriptor,
  PlayerRef,
  PlayerStatsSource,
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

export function createMockPlayerStatsGateway(): PlayerStatsGateway {
  let source: PlayerStatsSource = "deeplol";
  const capabilities = {
    builds: true,
    playerProfile: true,
    matchHistory: true,
    championStats: true,
    liveGame: false,
    directApi: true,
    siteRefresh: false,
    regions: ["KR", "JP1", "NA1"],
  };
  const maybeError = (player: PlayerRef) => {
    if (player.gameName.toLowerCase() === "missing") throw new Error("player-http:404");
    if (player.gameName.toLowerCase() === "invalid") {
      throw { kind: "validation", message: "invalid Riot ID" } satisfies PlayerViewError;
    }
    if (player.gameName.toLowerCase() === "limited") {
      throw new Error("player-http:429 retry-after=30");
    }
  };
  return {
    listSources: async () => [
      { id: "deeplol", label: "DeepLoL", capabilities },
      { id: "opgg", label: "OP.GG", capabilities },
    ],
    getSource: async () => source,
    setSource: async (next) => {
      if (next !== "deeplol" && next !== "opgg") throw new Error("Unsupported player provider");
      source = next;
    },
    profile: async (player) => {
      maybeError(player);
      return {
        source,
        identity: { ...player, puuid: `mock-${player.gameName}` },
        level: 920,
        profileIconId: 6,
        ranks: [
          {
            queue: "RANKED_SOLO_5x5",
            tier: "CHALLENGER",
            division: "I",
            lp: 1_234,
            wins: 91,
            losses: 63,
          },
          { queue: "RANKED_FLEX_SR", tier: "DIAMOND", division: "II", lp: 42, wins: 16, losses: 9 },
        ],
        previousSeasons: [
          { season: "25", queue: "RANKED_SOLO_5x5", tier: "MASTER", division: "I", lp: 225 },
          { season: "23", queue: "RANKED_SOLO_5x5", tier: "MASTER", division: "I", lp: 83 },
        ],
        ladderRank: 7,
        ladderPercentile: 0.01,
        fetchedAt: Date.now(),
        refresh: { appRefresh: true, siteRefresh: false },
        extras: { provider: source, data: {} },
      };
    },
    matches: async (player, cursor, _queue, forceRefresh) => {
      maybeError(player);
      const start = cursor ? Number(cursor) : 0;
      const count = 20;
      return {
        source,
        matches: Array.from({ length: count }, (_, index) => {
          const offset = start + index;
          const win = offset % 3 !== 1;
          return {
            matchId: `${source}-${offset}`,
            startedAt: Date.now() - offset * 3_600_000,
            durationSeconds: offset === 7 ? 240 : 1_600 + offset,
            queueId: offset % 4 === 0 ? 440 : 420,
            remake: offset === 7,
            championId: [103, 238, 64, 81][offset % 4],
            role: ["Middle", "Middle", "Jungle", "Bottom"][offset % 4],
            win,
            kills: 4 + (offset % 8),
            deaths: 2 + (offset % 6),
            assists: 5 + (offset % 11),
            cs: 170 + offset,
            items: [6655, 3020],
            spellIds: [4, 14],
            perkIds: [8112, 8139],
            participants: Array.from({ length: 10 }, (_entry, participant) => ({
              puuid: `p-${participant}`,
              gameName: `Player ${participant + 1}`,
              tagLine: "TEST",
              championId: [103, 238, 64, 81, 86, 24, 412, 22, 120, 517][participant],
              teamId: participant < 5 ? 100 : 200,
              role: ["Top", "Jungle", "Middle", "Bottom", "Supporter"][participant % 5],
              win: participant < 5 ? win : !win,
              kills: participant + 1,
              deaths: 3,
              assists: 7,
              items: [1001],
              extras: { provider: "none" as const },
            })),
            extras: { provider: "none" as const },
          };
        }),
        nextCursor: start === 0 ? "20" : undefined,
        partialFailures:
          player.gameName.toLowerCase() === "partial" && !forceRefresh
            ? [{ matchId: `${source}-failed`, message: "mock timeout", retryable: true }]
            : [],
        fetchedAt: Date.now(),
      };
    },
    championStats: async (player) => {
      maybeError(player);
      return [
        {
          championId: 103,
          games: 38,
          wins: 24,
          losses: 14,
          winRate: 24 / 38,
          kda: 4.12,
          csPerMinute: 7.8,
          role: "Middle",
        },
        {
          championId: 238,
          games: 21,
          wins: 11,
          losses: 10,
          winRate: 11 / 21,
          kda: 3.41,
          csPerMinute: 7.2,
          role: "Middle",
        },
        {
          championId: 64,
          games: 9,
          wins: 6,
          losses: 3,
          winRate: 6 / 9,
          kda: 5.2,
          csPerMinute: 6.4,
          role: "Jungle",
        },
      ].map((entry) => ({
        ...entry,
        source,
        queue: "RANKED_SOLO_5x5",
        extras: { provider: "none" as const },
      }));
    },
    refresh: async () => ({
      source,
      cacheInvalidated: true,
      mutationPerformed: false,
      refreshedAt: Date.now(),
    }),
  };
}

function defaultPlayerStatsGateway() {
  return new URLSearchParams(window.location.search).has("player-stats-mock")
    ? createMockPlayerStatsGateway()
    : tauriPlayerStatsGateway;
}

export type PlayerViewStatus = "idle" | "loading" | "ready" | "empty" | "error" | "partial";

export interface PlayerViewError {
  kind: "notFound" | "validation" | "rateLimited" | "timeout" | "invalidData" | "unknown";
  message: string;
  retryAfter?: number;
}

function classifyError(error: unknown): PlayerViewError {
  if (error && typeof error === "object") {
    const candidate = error as Partial<PlayerViewError>;
    if (
      typeof candidate.kind === "string" &&
      ["notFound", "validation", "rateLimited", "timeout", "invalidData", "unknown"].includes(
        candidate.kind,
      )
    ) {
      return {
        kind: candidate.kind as PlayerViewError["kind"],
        message: typeof candidate.message === "string" ? candidate.message : String(error),
        retryAfter: typeof candidate.retryAfter === "number" ? candidate.retryAfter : undefined,
      };
    }
  }
  const message = error instanceof Error ? error.message : String(error);
  if (/player-http:404|\b404\b/i.test(message)) return { kind: "notFound", message };
  if (/player-http:422|\b422\b/i.test(message)) return { kind: "validation", message };
  if (/player-http:429|\b429\b/i.test(message)) {
    const retry = message.match(/retry-after=([0-9]+)/i)?.[1];
    return { kind: "rateLimited", message, retryAfter: retry ? Number(retry) : undefined };
  }
  if (/player-timeout|\btimeout\b/i.test(message)) return { kind: "timeout", message };
  if (/invalid provider data|malformed json|invalid data/i.test(message)) {
    return { kind: "invalidData", message };
  }
  return { kind: "unknown", message };
}

function playerSource(value: string): PlayerStatsSource | undefined {
  return value === "deeplol" || value === "opgg" ? value : undefined;
}

function assertResponseSource(expected: PlayerStatsSource, actual: string, surface: string) {
  if (actual !== expected) {
    throw {
      kind: "invalidData",
      message: `${surface} returned ${actual || "an empty source"} while ${expected} was selected`,
    } satisfies PlayerViewError;
  }
}

function mergeMatchPages(current: MatchPage, next: MatchPage): MatchPage {
  if (next.source !== current.source) {
    throw {
      kind: "invalidData",
      message: `match pagination mixed ${current.source} and ${next.source}`,
    } satisfies PlayerViewError;
  }
  const matches = [...current.matches];
  const successfulIds = new Set(matches.map((match) => match.matchId));
  for (const match of next.matches) {
    if (!successfulIds.has(match.matchId)) {
      successfulIds.add(match.matchId);
      matches.push(match);
    }
  }
  const partialFailures = [] as MatchPage["partialFailures"];
  const failedIds = new Set<string>();
  for (const failure of [...current.partialFailures, ...next.partialFailures]) {
    if (!successfulIds.has(failure.matchId) && !failedIds.has(failure.matchId)) {
      failedIds.add(failure.matchId);
      partialFailures.push(failure);
    }
  }
  const madeProgress =
    matches.length > current.matches.length ||
    partialFailures.length > current.partialFailures.length;
  const nextCursor =
    madeProgress && next.nextCursor !== current.nextCursor ? next.nextCursor : undefined;
  return { ...next, matches, partialFailures, nextCursor };
}

export function createPlayerStatsState(gateway: PlayerStatsGateway = defaultPlayerStatsGateway()) {
  const [status, setStatus] = createSignal<PlayerViewStatus>("idle");
  const [sources, setSources] = createSignal<PlayerProviderDescriptor[]>([]);
  const [source, setSourceState] = createSignal<PlayerStatsSource>("deeplol");
  const [player, setPlayer] = createSignal<PlayerRef>();
  const [profile, setProfile] = createSignal<PlayerProfile>();
  const [matches, setMatches] = createSignal<MatchPage>();
  const [championStats, setChampionStats] = createSignal<PlayerChampionStats[]>([]);
  const [error, setError] = createSignal<PlayerViewError>();
  const [loadingMore, setLoadingMore] = createSignal(false);
  let generation = 0;
  let sourceSelection = 0;
  let pendingSource: PlayerStatsSource | undefined;
  let sourceQueue: Promise<void> = Promise.resolve();
  let queueFilter: number | undefined;
  let championFilters: { season?: string; queue?: string; role?: string } = {};

  async function initialize() {
    const [available, active] = await Promise.all([gateway.listSources(), gateway.getSource()]);
    const supported = available.filter(
      (entry) => ["deeplol", "opgg"].includes(entry.id) && entry.capabilities.playerProfile,
    );
    setSources(supported);
    const parsed = playerSource(active);
    if (parsed && supported.some((entry) => entry.id === parsed)) setSourceState(parsed);
  }

  async function search(nextPlayer: PlayerRef, forceRefresh = false) {
    const request = ++generation;
    setPlayer(nextPlayer);
    setStatus("loading");
    setError(undefined);
    setProfile(undefined);
    setMatches(undefined);
    setChampionStats([]);
    try {
      const expectedSource = source();
      const nextProfile = await gateway.profile(nextPlayer, forceRefresh);
      if (request !== generation) return;
      assertResponseSource(expectedSource, nextProfile.source, "profile");
      const [nextMatches, nextChampions] = await Promise.all([
        gateway.matches(nextPlayer, undefined, queueFilter, forceRefresh),
        gateway.championStats(nextPlayer, championFilters, forceRefresh),
      ]);
      if (request !== generation) return;
      assertResponseSource(expectedSource, nextMatches.source, "matches");
      for (const champion of nextChampions) {
        assertResponseSource(expectedSource, champion.source, "champion stats");
      }
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
    const parsed = playerSource(nextSource);
    const descriptor = sources().find((entry) => entry.id === nextSource);
    if (!parsed || !descriptor?.capabilities.playerProfile) {
      throw new Error("Unsupported player provider");
    }
    if (parsed === source() && pendingSource === undefined) return;
    const selection = ++sourceSelection;
    pendingSource = parsed;
    generation += 1;
    setStatus("loading");
    setError(undefined);
    setProfile(undefined);
    setMatches(undefined);
    setChampionStats([]);
    const operation = sourceQueue.then(() => gateway.setSource(parsed));
    sourceQueue = operation.catch(() => undefined);
    try {
      await operation;
      if (selection !== sourceSelection) return;
      pendingSource = undefined;
      setSourceState(parsed);
      const current = player();
      if (current) await search(current);
      else setStatus("idle");
    } catch (cause) {
      if (selection !== sourceSelection) return;
      pendingSource = undefined;
      setError(classifyError(cause));
      setStatus("error");
    }
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
      assertResponseSource(source(), next.source, "matches");
      const merged = mergeMatchPages(page, next);
      setMatches(merged);
      setError(undefined);
      setStatus(merged.partialFailures.length > 0 ? "partial" : "ready");
    } catch (cause) {
      if (request === generation) {
        setError(classifyError(cause));
        setStatus("error");
      }
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
    const request = ++generation;
    setStatus("loading");
    setError(undefined);
    setProfile(undefined);
    setMatches(undefined);
    setChampionStats([]);
    try {
      await gateway.refresh(current);
      if (request !== generation) return;
      await search(current, true);
    } catch (cause) {
      if (request !== generation) return;
      setError(classifyError(cause));
      setStatus("error");
    }
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
