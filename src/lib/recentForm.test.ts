import { describe, expect, it } from "vitest";
import { formSummary } from "./recentForm";

const game = (win: boolean, kills = 5, deaths = 2, assists = 7) => ({
  championId: 1,
  win,
  kills,
  deaths,
  assists,
  queueId: 420,
  gameCreation: 0,
});

describe("formSummary", () => {
  it("returns null without games", () => {
    expect(formSummary([])).toBeNull();
  });

  it("aggregates record and KDA", () => {
    const s = formSummary([game(true), game(false)]);
    expect(s?.record).toBe("1W 1L");
    expect(s?.kda).toBe("KDA 6.00");
  });

  it("reports Perfect KDA when deathless", () => {
    expect(formSummary([game(true, 3, 0, 4)])?.kda).toBe("KDA Perfect");
  });

  it("labels streaks from two and colors them from three", () => {
    const two = formSummary([game(true), game(true), game(false)]);
    expect(two?.streakLabel).toBe("2連勝");
    expect(two?.streakWin).toBe(false);

    const three = formSummary([game(false), game(false), game(false)]);
    expect(three?.streakLabel).toBe("3連敗");
    expect(three?.streakLoss).toBe(true);
  });
});
