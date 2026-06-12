import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import type { Settings } from "../types";

const [autoImport, setAutoImport] = createSignal(true);
const [importSpells, setImportSpellsState] = createSignal(true);
const [spellsFlipped, setSpellsFlippedState] = createSignal(false);
const [pinned, setPinnedState] = createSignal(false);
const [dataSource, setDataSourceState] = createSignal("deeplol");
const [dataSources, setDataSources] = createSignal<string[]>(["deeplol"]);
/** Gear toggles hidden state (starts hidden like the original DOM). */
const [gearHidden, setGearHidden] = createSignal(true);

export {
  autoImport,
  setAutoImport,
  importSpells,
  spellsFlipped,
  pinned,
  dataSource,
  dataSources,
  gearHidden,
  setGearHidden,
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

export function togglePinned() {
  setPinned(!pinned());
}

export function setDataSource(kind: string) {
  setDataSourceState(kind);
  invoke("set_data_source", { kind }).catch(() => {});
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
