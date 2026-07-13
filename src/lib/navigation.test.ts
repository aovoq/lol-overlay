import { describe, expect, it } from "vitest";
import { type AutomaticNavigationState, automaticRoute } from "./navigation";

const base: AutomaticNavigationState = {
  champSelectActive: false,
  inGame: false,
  routedDraft: false,
  routedInGame: false,
  autoOpenDraft: true,
  autoOpenLive: true,
};

describe("automaticRoute", () => {
  it("opens the draft board once when champ select starts", () => {
    expect(automaticRoute({ ...base, champSelectActive: true })).toBe("/draft");
    expect(automaticRoute({ ...base, champSelectActive: true, routedDraft: true })).toBeNull();
  });

  it("does not open the draft board when disabled", () => {
    expect(automaticRoute({ ...base, champSelectActive: true, autoOpenDraft: false })).toBeNull();
  });

  it("opens live once and gives it priority", () => {
    expect(automaticRoute({ ...base, champSelectActive: true, inGame: true })).toBe("/live");
    expect(automaticRoute({ ...base, inGame: true, routedInGame: true })).toBeNull();
    expect(automaticRoute({ ...base, inGame: true, autoOpenLive: false })).toBeNull();
  });

  it("stays put when nothing changed", () => {
    expect(automaticRoute(base)).toBeNull();
  });
});
