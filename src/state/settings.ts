import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import type { PlayerProviderDescriptor, PresentationMode, Settings } from "../types";

export type ThemeMode = "dark" | "light";

const THEME_STORAGE_KEY = "lol-overlay.theme";
const THEME_MODES: ThemeMode[] = ["dark", "light"];

function storedTheme(): ThemeMode {
  const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  return THEME_MODES.includes(stored as ThemeMode) ? (stored as ThemeMode) : "light";
}

function applyTheme(mode: ThemeMode) {
  document.documentElement.dataset.theme = mode;
  document.documentElement.style.colorScheme = mode;
}

const [autoImport, setAutoImport] = createSignal(true);
const [importSpells, setImportSpellsState] = createSignal(true);
const [spellsFlipped, setSpellsFlippedState] = createSignal(false);
const [dataSource, setDataSourceState] = createSignal("deeplol");
const [dataSources, setDataSources] = createSignal<string[]>(["deeplol"]);
const [playerStatsSource, setPlayerStatsSourceState] = createSignal("deeplol");
const [playerStatsSources, setPlayerStatsSources] = createSignal<PlayerProviderDescriptor[]>([]);
const [presentationMode, setPresentationModeState] = createSignal<PresentationMode>("overlay");
const [themeMode, setThemeModeState] = createSignal<ThemeMode>(storedTheme());
const [developerMode, setDeveloperModeState] = createSignal(false);
const [autoOpenDraft, setAutoOpenDraftState] = createSignal(true);
const [autoOpenLive, setAutoOpenLiveState] = createSignal(true);

applyTheme(themeMode());

export {
  autoImport,
  autoOpenDraft,
  autoOpenLive,
  dataSource,
  dataSources,
  developerMode,
  importSpells,
  playerStatsSource,
  playerStatsSources,
  presentationMode,
  setAutoImport,
  spellsFlipped,
  themeMode,
};

// Persisted under the historical `autoOpenChampion` settings key; the UI now
// opens the draft board instead of a champion page.
export function setAutoOpenDraft(enabled: boolean) {
  setAutoOpenDraftState(enabled);
  invoke("set_auto_open_champion", { enabled }).catch(() => {});
}

export function setAutoOpenLive(enabled: boolean) {
  setAutoOpenLiveState(enabled);
  invoke("set_auto_open_live", { enabled }).catch(() => {});
}

export function setImportSpells(on: boolean) {
  setImportSpellsState(on);
  invoke("set_import_spells", { enabled: on }).catch(() => {});
}

export function setSpellsFlipped(flipped: boolean) {
  setSpellsFlippedState(flipped);
  invoke("set_spells_flipped", { flipped }).catch(() => {});
}

export function setDataSource(kind: string) {
  setDataSourceState(kind);
  invoke("set_data_source", { kind }).catch(() => {});
}

export function setPlayerStatsSource(source: string) {
  setPlayerStatsSourceState(source);
  invoke("set_player_stats_source", { source }).catch(() => {});
}

export function setDeveloperMode(enabled: boolean) {
  setDeveloperModeState(enabled);
  invoke("set_developer_mode", { enabled }).catch(() => {});
}

export function setPresentationMode(mode: PresentationMode) {
  setPresentationModeState(mode);
  invoke("set_presentation_mode", { mode }).catch(() => {});
}

export function setThemeMode(mode: ThemeMode) {
  setThemeModeState(mode);
  applyTheme(mode);
  window.localStorage.setItem(THEME_STORAGE_KEY, mode);
}

export function applySettings(s: Partial<Settings>) {
  if (s.autoImportRunes !== undefined) setAutoImport(s.autoImportRunes);
  if (s.importSpells !== undefined) setImportSpellsState(s.importSpells);
  if (s.spellsFlipped !== undefined) setSpellsFlippedState(s.spellsFlipped);
  if (s.buildDataSource !== undefined) setDataSourceState(s.buildDataSource);
  if (s.playerStatsSource !== undefined) setPlayerStatsSourceState(s.playerStatsSource);
  if (s.presentationMode !== undefined) setPresentationModeState(s.presentationMode);
  if (s.developerMode !== undefined) setDeveloperModeState(s.developerMode);
  if (s.autoOpenChampion !== undefined) setAutoOpenDraftState(s.autoOpenChampion);
  if (s.autoOpenLive !== undefined) setAutoOpenLiveState(s.autoOpenLive);
}

invoke<Settings>("get_settings")
  .then((s) => applySettings(s))
  .catch(() => {});

invoke<string[]>("list_data_sources")
  .then((sources) => {
    if (sources.length > 0) setDataSources(sources);
  })
  .catch(() => {});

invoke<string>("get_data_source")
  .then((kind) => setDataSourceState(kind))
  .catch(() => {});

invoke<PlayerProviderDescriptor[]>("list_player_stats_sources")
  .then((sources) =>
    setPlayerStatsSources(sources.filter((source) => source.capabilities.playerProfile)),
  )
  .catch(() => {});

invoke<string>("get_player_stats_source")
  .then((source) => setPlayerStatsSourceState(source))
  .catch(() => {});

listen<string>("data-source", (event) => setDataSourceState(event.payload)).catch(() => {});
listen<string>("player-stats-source", (event) => setPlayerStatsSourceState(event.payload)).catch(
  () => {},
);
