import { describe, expect, it } from "vitest";
import type {
  MatchPage,
  PlayerChampionStats,
  PlayerProfile,
  PlayerProviderDescriptor,
  PlayerRef,
} from "../types";
import { createPlayerStatsState, type PlayerStatsGateway } from "./playerStats";

const player: PlayerRef = { platformId: "KR", gameName: "Faker", tagLine: "KR1" };

const profile = (source: string, gameName = player.gameName): PlayerProfile =>
  ({
    source,
    identity: { ...player, gameName, puuid: null },
    ranks: [],
    previousSeasons: [],
    fetchedAt: 1,
    refresh: { appRefresh: true, siteRefresh: false },
    extras: { provider: "none" },
  }) as PlayerProfile;
const page = (source: string, ids: string[], nextCursor?: string): MatchPage =>
  ({
    source,
    matches: ids.map((matchId) => ({
      matchId,
      startedAt: 1,
      durationSeconds: 1,
      queueId: 420,
      remake: false,
      championId: 1,
      win: true,
      kills: 1,
      deaths: 1,
      assists: 1,
      items: [],
      spellIds: [],
      perkIds: [],
      participants: [],
      extras: { provider: "none" },
    })),
    nextCursor,
    partialFailures: [],
    fetchedAt: 1,
  }) as MatchPage;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function gateway(overrides: Partial<PlayerStatsGateway> = {}): PlayerStatsGateway {
  let source = "deeplol";
  const descriptor = (id: string): PlayerProviderDescriptor =>
    ({ id, label: id, capabilities: { playerProfile: true } }) as PlayerProviderDescriptor;
  return {
    listSources: async () => [descriptor("deeplol"), descriptor("opgg"), descriptor("ugg")],
    getSource: async () => source,
    setSource: async (next) => {
      source = next;
    },
    profile: async () => profile(source),
    matches: async (_player, cursor) =>
      page(source, [cursor ?? "first"], cursor ? undefined : "20"),
    championStats: async () => [] as PlayerChampionStats[],
    refresh: async () => ({
      source,
      cacheInvalidated: true,
      mutationPerformed: false,
      refreshedAt: 1,
    }),
    ...overrides,
  };
}

describe("player stats state", () => {
  it("prevents a stale search from replacing a newer one", async () => {
    const slow = deferred<PlayerProfile>();
    let calls = 0;
    const state = createPlayerStatsState(
      gateway({
        profile: async () => {
          calls += 1;
          return calls === 1 ? slow.promise : profile("deeplol", "New");
        },
      }),
    );
    const first = state.search(player);
    const secondPlayer = { ...player, gameName: "New" };
    await state.search(secondPlayer);
    slow.resolve(profile("deeplol", "Old"));
    await first;
    expect(state.profile()?.identity.gameName).toBe("New");
    expect(state.player()).toEqual(secondPlayer);
  });

  it("switches providers and reloads the same player", async () => {
    const state = createPlayerStatsState(gateway());
    await state.initialize();
    await state.search(player);
    await state.selectSource("opgg");
    expect(state.source()).toBe("opgg");
    expect(state.player()).toEqual(player);
    expect(state.profile()?.source).toBe("opgg");
  });

  it("appends cursor pages and ignores duplicate load-more clicks", async () => {
    const more = deferred<MatchPage>();
    const api = gateway({
      matches: async (_player, cursor) => (cursor ? more.promise : page("deeplol", ["a"], "20")),
    });
    const state = createPlayerStatsState(api);
    await state.search(player);
    const first = state.loadMore();
    const duplicate = state.loadMore();
    more.resolve(page("deeplol", ["b"]));
    await Promise.all([first, duplicate]);
    expect(state.matches()?.matches.map((match) => match.matchId)).toEqual(["a", "b"]);
  });

  it("deduplicates overlapping pages and stops a repeated cursor loop", async () => {
    const state = createPlayerStatsState(
      gateway({
        matches: async (_player, cursor) =>
          cursor ? page("deeplol", ["a", "b"], "20") : page("deeplol", ["a"], "20"),
      }),
    );
    await state.search(player);
    await state.loadMore();
    expect(state.matches()?.matches.map((match) => match.matchId)).toEqual(["a", "b"]);
    expect(state.matches()?.nextCursor).toBeUndefined();
  });

  it("rejects mixed-provider responses before publishing partial state", async () => {
    const state = createPlayerStatsState(
      gateway({ matches: async () => page("opgg", ["foreign"]) }),
    );
    await state.search(player);
    expect(state.status()).toBe("error");
    expect(state.error()).toMatchObject({ kind: "invalidData" });
    expect(state.profile()).toBeUndefined();
    expect(state.matches()).toBeUndefined();
  });

  it("orders overlapping provider selections so the last source wins", async () => {
    const first = deferred<void>();
    const persisted: string[] = [];
    let calls = 0;
    const state = createPlayerStatsState(
      gateway({
        setSource: async (next) => {
          calls += 1;
          if (calls === 1) await first.promise;
          persisted.push(next);
        },
      }),
    );
    await state.initialize();
    const opgg = state.selectSource("opgg");
    const deeplol = state.selectSource("deeplol");
    first.resolve();
    await Promise.all([opgg, deeplol]);
    expect(persisted).toEqual(["opgg", "deeplol"]);
    expect(state.source()).toBe("deeplol");
  });

  it("classifies 404 and 429 states", async () => {
    const missing = createPlayerStatsState(
      gateway({ profile: async () => Promise.reject(new Error("player-http:404")) }),
    );
    await missing.search(player);
    expect(missing.error()?.kind).toBe("notFound");

    const limited = createPlayerStatsState(
      gateway({
        profile: async () => Promise.reject(new Error("player-http:429 retry-after=12")),
      }),
    );
    await limited.search(player);
    expect(limited.error()).toMatchObject({ kind: "rateLimited", retryAfter: 12 });
  });

  it("uses typed command errors and clears data from the previous player", async () => {
    const state = createPlayerStatsState(
      gateway({
        profile: async (target) => {
          if (target.gameName === "Invalid") {
            throw { kind: "validation", message: "invalid Riot ID" };
          }
          return profile("deeplol");
        },
      }),
    );
    await state.search(player);
    expect(state.profile()).toBeDefined();
    await state.search({ ...player, gameName: "Invalid" });
    expect(state.error()).toEqual({
      kind: "validation",
      message: "invalid Riot ID",
      retryAfter: undefined,
    });
    expect(state.profile()).toBeUndefined();
  });

  it("surfaces load-more and refresh failures instead of silently keeping ready state", async () => {
    const loadFailure = createPlayerStatsState(
      gateway({
        matches: async (_player, cursor) => {
          if (cursor) throw { kind: "rateLimited", message: "slow down", retryAfter: 5 };
          return page("deeplol", ["a"], "20");
        },
      }),
    );
    await loadFailure.search(player);
    await loadFailure.loadMore();
    expect(loadFailure.status()).toBe("error");
    expect(loadFailure.error()?.kind).toBe("rateLimited");

    const refreshFailure = createPlayerStatsState(
      gateway({ refresh: async () => Promise.reject(new Error("refresh unavailable")) }),
    );
    await refreshFailure.search(player);
    await refreshFailure.refresh();
    expect(refreshFailure.status()).toBe("error");
    expect(refreshFailure.error()?.message).toContain("refresh unavailable");
  });

  it("isolates idle, loading, ready, empty, and partial transitions", async () => {
    const slow = deferred<PlayerProfile>();
    const loading = createPlayerStatsState(gateway({ profile: async () => slow.promise }));
    expect(loading.status()).toBe("idle");
    const search = loading.search(player);
    expect(loading.status()).toBe("loading");
    slow.resolve(profile("deeplol"));
    await search;
    expect(loading.status()).toBe("ready");

    const empty = createPlayerStatsState(
      gateway({ matches: async () => page("deeplol", []), championStats: async () => [] }),
    );
    await empty.search(player);
    expect(empty.status()).toBe("empty");

    const partialPage = page("deeplol", ["ok"]);
    partialPage.partialFailures = [{ matchId: "bad", message: "hydrate failed", retryable: true }];
    const partial = createPlayerStatsState(gateway({ matches: async () => partialPage }));
    await partial.search(player);
    expect(partial.status()).toBe("partial");
  });

  it("filters build-only sources and covers every frontend error kind", async () => {
    const initialized = createPlayerStatsState(gateway());
    await initialized.initialize();
    expect(initialized.sources().map((source) => source.id)).toEqual(["deeplol", "opgg"]);
    await expect(initialized.selectSource("ugg")).rejects.toThrow("Unsupported player provider");

    for (const [failure, kind] of [
      [{ kind: "validation", message: "bad input" }, "validation"],
      [{ kind: "timeout", message: "slow upstream" }, "timeout"],
      [{ kind: "invalidData", message: "bad schema" }, "invalidData"],
      [new Error("player-timeout"), "timeout"],
      [new Error("invalid provider data: malformed json"), "invalidData"],
      [new Error("unexpected"), "unknown"],
    ] as const) {
      const state = createPlayerStatsState(
        gateway({ profile: async () => Promise.reject(failure) }),
      );
      await state.search(player);
      expect(state.error()?.kind).toBe(kind);
    }
  });

  it("prevents stale load-more and refresh completions from replacing a new search", async () => {
    const oldPage = deferred<MatchPage>();
    const refreshed = deferred<Awaited<ReturnType<PlayerStatsGateway["refresh"]>>>();
    const state = createPlayerStatsState(
      gateway({
        matches: async (target, cursor) => {
          if (cursor) return oldPage.promise;
          return page(
            "deeplol",
            [target.gameName],
            target.gameName === player.gameName ? "20" : undefined,
          );
        },
        refresh: async () => refreshed.promise,
      }),
    );
    await state.search(player);
    const loadMore = state.loadMore();
    const refresh = state.refresh();
    const replacement = { ...player, gameName: "Replacement" };
    await state.search(replacement);
    oldPage.resolve(page("deeplol", ["stale"]));
    refreshed.resolve({
      source: "deeplol",
      cacheInvalidated: true,
      mutationPerformed: false,
      refreshedAt: 2,
    });
    await Promise.all([loadMore, refresh]);
    expect(state.player()).toEqual(replacement);
    expect(state.matches()?.matches.map((match) => match.matchId)).toEqual(["Replacement"]);
    expect(state.status()).toBe("ready");
  });

  it("reloads with isolated queue and champion filters", async () => {
    const queueCalls: Array<number | undefined> = [];
    const championCalls: Array<{ season?: string; queue?: string; role?: string }> = [];
    const state = createPlayerStatsState(
      gateway({
        matches: async (_target, _cursor, queue) => {
          queueCalls.push(queue);
          return page("deeplol", [String(queue ?? "all")]);
        },
        championStats: async (_target, filters) => {
          championCalls.push(filters);
          return [];
        },
      }),
    );
    await state.search(player);
    await state.setQueueFilter(1700);
    await state.setChampionFilters({ season: "15", queue: "RANKED", role: "Middle" });
    expect(queueCalls[queueCalls.length - 1]).toBe(1700);
    expect(championCalls[championCalls.length - 1]).toEqual({
      season: "15",
      queue: "RANKED",
      role: "Middle",
    });
  });
});
