import { describe, expect, it } from "vitest";
import { dataSourceLabel, fmtTier, phaseChipLabel, roleLabel } from "./openlol";

describe("openlol formatters", () => {
  it("formats phase chips", () => {
    expect(phaseChipLabel({ phase: "ChampSelect", clientUp: true, inGame: false })).toBe(
      "CHAMP SELECT",
    );
    expect(phaseChipLabel({ phase: "", clientUp: false, inGame: false })).toBe("OFFLINE");
  });

  it("formats role labels", () => {
    expect(roleLabel("middle")).toBe("MID");
    expect(roleLabel("utility")).toBe("SUPPORT");
    expect(roleLabel("custom")).toBe("CUSTOM");
  });

  it("formats tiers", () => {
    expect(fmtTier("EMERALD")).toBe("Emerald");
  });

  it("labels the LOL.PS build provider", () => {
    expect(dataSourceLabel("lolps")).toBe("LOL.PS");
  });
});
