import { createSignal } from "solid-js";
import type { PlayerProviderDescriptor } from "../types";

const PLAYER_SOURCE_IDS = new Set(["deeplol", "opgg"]);

export interface PlayerSettingsGateway {
  listSources(): Promise<PlayerProviderDescriptor[]>;
  getSource(): Promise<string>;
  setSource(source: string): Promise<void>;
  onSource(handler: (source: string) => void): Promise<() => void> | void;
}

export function createPlayerSettingsController(gateway: PlayerSettingsGateway) {
  const [source, setSourceState] = createSignal("deeplol");
  const [sources, setSources] = createSignal<PlayerProviderDescriptor[]>([]);

  const isAllowed = (candidate: string) =>
    PLAYER_SOURCE_IDS.has(candidate) && sources().some((entry) => entry.id === candidate);

  function applySource(candidate: string) {
    if (PLAYER_SOURCE_IDS.has(candidate)) setSourceState(candidate);
  }

  async function initialize() {
    const [available, active] = await Promise.all([gateway.listSources(), gateway.getSource()]);
    const supported = available.filter(
      (entry) => PLAYER_SOURCE_IDS.has(entry.id) && entry.capabilities.playerProfile,
    );
    setSources(supported);
    if (supported.some((entry) => entry.id === active)) setSourceState(active);
    await gateway.onSource((nextSource) => {
      if (PLAYER_SOURCE_IDS.has(nextSource)) setSourceState(nextSource);
    });
  }

  async function selectSource(nextSource: string) {
    if (nextSource === source()) return;
    if (!isAllowed(nextSource)) throw new Error("Unsupported player provider");
    const previous = source();
    setSourceState(nextSource);
    try {
      await gateway.setSource(nextSource);
    } catch (error) {
      setSourceState(previous);
      throw error;
    }
  }

  return { source, sources, initialize, selectSource, applySource };
}
