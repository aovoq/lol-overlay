import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import type { Settings } from "../types";

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
const [pinned, setPinnedState] = createSignal(false);
const [dataSource, setDataSourceState] = createSignal("deeplol");
const [dataSources, setDataSources] = createSignal<string[]>(["deeplol"]);
const [themeMode, setThemeModeState] = createSignal<ThemeMode>(storedTheme());

applyTheme(themeMode());

export {
  autoImport,
  dataSource,
  dataSources,
  importSpells,
  pinned,
  setAutoImport,
  spellsFlipped,
  themeMode,
};

export function setImportSpells(on: boolean) {
  setImportSpellsState(on);
  invoke("set_import_spells", { enabled: on }).catch(() => {});
}

export function setSpellsFlipped(flipped: boolean) {
  setSpellsFlippedState(flipped);
  invoke("set_spells_flipped", { flipped }).catch(() => {});
}

export function setPinned(on: boolean) {
  setPinnedState(on);
  invoke("set_pinned", { pinned: on }).catch(() => {});
}

export function setDataSource(kind: string) {
  setDataSourceState(kind);
  invoke("set_data_source", { kind }).catch(() => {});
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
  if (s.pinned !== undefined) setPinnedState(s.pinned);
  if (s.dataSource !== undefined) setDataSourceState(s.dataSource);
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
