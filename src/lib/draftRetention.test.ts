import { describe, expect, it } from "vitest";
import { retainDraft } from "./draftRetention";

const phase = (phase: string, inGame = false) => ({ phase, clientUp: true, inGame });

describe("retainDraft", () => {
  it("keeps the draft through the load screen and the game", () => {
    expect(retainDraft(phase("ChampSelect"))).toBe(true);
    expect(retainDraft(phase("InProgress"))).toBe(true); // load screen, API not up yet
    expect(retainDraft(phase("InProgress", true))).toBe(true);
  });

  it("keeps the draft if the game outlives the LCU phase", () => {
    expect(retainDraft(phase("Other", true))).toBe(true);
  });

  it("drops the draft on a dodge or back in the lobby", () => {
    expect(retainDraft(phase("Lobby"))).toBe(false);
    expect(retainDraft(phase("Matchmaking"))).toBe(false);
    expect(retainDraft(phase("None"))).toBe(false);
    expect(retainDraft(phase("Other"))).toBe(false);
  });
});
