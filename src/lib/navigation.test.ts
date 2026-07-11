import { describe, expect, it } from "vitest";
import { type AutomaticNavigationState, automaticRoute } from "./navigation";

const base: AutomaticNavigationState = {
  championId: 0,
  championLocked: false,
  inGame: false,
  routedChampion: 0,
  routedInGame: false,
  autoOpenChampion: true,
  autoOpenLive: true,
};

describe("automaticRoute", () => {
  it("opens a newly locked champion once", () => {
    expect(automaticRoute({ ...base, championId: 103, championLocked: true })).toBe(
      "/champions/103",
    );
    expect(
      automaticRoute({
        ...base,
        championId: 103,
        championLocked: true,
        routedChampion: 103,
      }),
    ).toBeNull();
  });

  it("does not navigate for hover or when disabled", () => {
    expect(automaticRoute({ ...base, championId: 103 })).toBeNull();
    expect(
      automaticRoute({
        ...base,
        championId: 103,
        championLocked: true,
        autoOpenChampion: false,
      }),
    ).toBeNull();
  });

  it("opens live once and gives it priority", () => {
    expect(automaticRoute({ ...base, championId: 103, championLocked: true, inGame: true })).toBe(
      "/live",
    );
    expect(automaticRoute({ ...base, inGame: true, routedInGame: true })).toBeNull();
    expect(automaticRoute({ ...base, inGame: true, autoOpenLive: false })).toBeNull();
  });
});
