import { describe, expect, it } from "vitest";
import type { PlayerProviderDescriptor } from "../types";
import { createPlayerSettingsController, type PlayerSettingsGateway } from "./playerSettings";

const descriptor = (id: string, playerProfile = true) =>
  ({ id, label: id, capabilities: { playerProfile } }) as PlayerProviderDescriptor;

function gateway(overrides: Partial<PlayerSettingsGateway> = {}) {
  let event: ((source: string) => void) | undefined;
  const persisted: string[] = [];
  const api: PlayerSettingsGateway = {
    listSources: async () => [
      descriptor("deeplol"),
      descriptor("opgg"),
      descriptor("ugg"),
      descriptor("lolalytics", false),
    ],
    getSource: async () => "deeplol",
    setSource: async (source) => {
      persisted.push(source);
    },
    onSource: (handler) => {
      event = handler;
    },
    ...overrides,
  };
  return { api, persisted, emit: (source: string) => event?.(source) };
}

describe("Player settings", () => {
  it("exposes only the two registered Player providers and persists selection", async () => {
    const fixture = gateway();
    const state = createPlayerSettingsController(fixture.api);
    await state.initialize();
    expect(state.sources().map((source) => source.id)).toEqual(["deeplol", "opgg"]);
    await state.selectSource("opgg");
    expect(fixture.persisted).toEqual(["opgg"]);
    expect(state.source()).toBe("opgg");
    await expect(state.selectSource("ugg")).rejects.toThrow("Unsupported player provider");
  });

  it("tracks the player-stats-source event and ignores build-only U.GG events", async () => {
    const fixture = gateway({ getSource: async () => "opgg" });
    const state = createPlayerSettingsController(fixture.api);
    await state.initialize();
    expect(state.source()).toBe("opgg");
    fixture.emit("deeplol");
    expect(state.source()).toBe("deeplol");
    fixture.emit("ugg");
    expect(state.source()).toBe("deeplol");
  });

  it("rolls back optimistic state when persistence fails", async () => {
    const fixture = gateway({ setSource: async () => Promise.reject(new Error("disk full")) });
    const state = createPlayerSettingsController(fixture.api);
    await state.initialize();
    await expect(state.selectSource("opgg")).rejects.toThrow("disk full");
    expect(state.source()).toBe("deeplol");
  });
});
