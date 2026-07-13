import { describe, expect, it } from "vitest";
import type { PlayerMatch, PlayerRef } from "../types";
import {
  addPlayerHistory,
  filterMatches,
  loadPlayerHistory,
  PLAYER_HISTORY_KEY,
  PLAYER_HISTORY_LIMIT,
  parseRiotId,
  summarizeMatches,
} from "./playerSearch";

describe("player search", () => {
  it("requires a tag while preserving internal spaces and case", () => {
    expect(parseRiotId("jp1", "  Hide on Bush#Jp 1  ")).toEqual({
      platformId: "JP1",
      gameName: "Hide on Bush",
      tagLine: "Jp 1",
    });
    expect(() => parseRiotId("JP1", "NoTag")).toThrow(/GameName#Tag/);
    expect(() => parseRiotId("JP1", "#JP1")).toThrow(/GameName#Tag/);
  });

  it("deduplicates exact identities and caps history at ten", () => {
    let history: PlayerRef[] = [];
    for (let index = 0; index < PLAYER_HISTORY_LIMIT + 2; index += 1) {
      history = addPlayerHistory(history, {
        platformId: "JP1",
        gameName: `Player ${index}`,
        tagLine: "JP1",
      });
    }
    expect(history).toHaveLength(PLAYER_HISTORY_LIMIT);
    expect(history[0].gameName).toBe("Player 11");
    expect(history[history.length - 1]?.gameName).toBe("Player 2");
    expect(addPlayerHistory(history, history[4])[0]).toEqual(history[4]);
  });

  it("drops corrupt storage safely", () => {
    let removed = false;
    const storage = {
      getItem: (key: string) => (key === PLAYER_HISTORY_KEY ? "{" : null),
      removeItem: () => {
        removed = true;
      },
    };
    expect(loadPlayerHistory(storage)).toEqual([]);
    expect(removed).toBe(true);
  });
});

describe("player match filters", () => {
  const match = (queueId: number, win: boolean, remake = false) =>
    ({ queueId, win, remake }) as PlayerMatch;
  const matches = [match(420, true), match(420, false), match(440, true, true)];

  it("filters queues without changing all-queue results", () => {
    expect(filterMatches(matches)).toHaveLength(3);
    expect(filterMatches(matches, 420)).toHaveLength(2);
  });

  it("shows remakes but excludes them from aggregate results", () => {
    expect(summarizeMatches(matches)).toEqual({ games: 2, wins: 1, losses: 1, winRate: 0.5 });
    expect(summarizeMatches([match(440, false, true)]).winRate).toBeUndefined();
  });
});
