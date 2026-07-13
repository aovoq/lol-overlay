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

const profile = (source: string): PlayerProfile =>
  ({
    source,
    identity: { ...player, puuid: null },
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
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function gateway(overrides: Partial<PlayerStatsGateway> = {}): PlayerStatsGateway {
  let source = "deeplol";
  const descriptor = (id: string): PlayerProviderDescriptor =>
    ({ id, label: id, capabilities: { playerProfile: true } }) as PlayerProviderDescriptor;
  return {
    listSources: async () => [descriptor("deeplol"), descriptor("ugg")],
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
          return calls === 1 ? slow.promise : profile("new");
        },
      }),
    );
    const first = state.search(player);
    const secondPlayer = { ...player, gameName: "New" };
    await state.search(secondPlayer);
    slow.resolve(profile("old"));
    await first;
    expect(state.profile()?.source).toBe("new");
    expect(state.player()).toEqual(secondPlayer);
  });

  it("switches providers and reloads the same player", async () => {
    const state = createPlayerStatsState(gateway());
    await state.initialize();
    await state.search(player);
    await state.selectSource("ugg");
    expect(state.source()).toBe("ugg");
    expect(state.player()).toEqual(player);
    expect(state.profile()?.source).toBe("ugg");
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
});
