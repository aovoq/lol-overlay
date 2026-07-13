import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";

export type MockStage = "off" | "champselect" | "ingame";

export interface DebugLogEntry {
  id: number;
  time: string;
  event: string;
  payload: unknown;
}

const [mockStage, setMockStage] = createSignal<MockStage>("off");
const [eventLog, setEventLog] = createSignal<DebugLogEntry[]>([]);

export { eventLog, mockStage };

export function selectMockStage(stage: MockStage) {
  // The backend echoes `mock-stage`, which updates the signal.
  invoke("set_mock_stage", { stage }).catch(() => {});
}

export function clearEventLog() {
  setEventLog([]);
}

invoke<MockStage>("get_mock_stage")
  .then(setMockStage)
  .catch(() => {});

listen<MockStage>("mock-stage", (e) => setMockStage(e.payload)).catch(() => {});

// Mirror every backend event into the debug log (newest first, capped).
const MIRRORED_EVENTS = [
  "phase",
  "champ-select",
  "recommendations",
  "summoner",
  "match-history",
  "lp-change",
  "rune-imported",
  "window-mode",
  "interactive",
  "log",
  "data-source",
  "player-stats-source",
  "mock-stage",
];
const MAX_LOG_ENTRIES = 200;
let nextId = 0;

for (const event of MIRRORED_EVENTS) {
  listen<unknown>(event, (e) => {
    const entry: DebugLogEntry = {
      id: nextId++,
      time: new Date().toLocaleTimeString("ja-JP", { hour12: false }),
      event,
      payload: e.payload,
    };
    setEventLog((log) => [entry, ...log].slice(0, MAX_LOG_ENTRIES));
  }).catch(() => {});
}
