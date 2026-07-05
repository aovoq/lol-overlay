import { For } from "solid-js";
import {
  clearEventLog,
  eventLog,
  type MockStage,
  mockStage,
  selectMockStage,
  setPlaygroundOpen,
} from "../state/debug";

/** Debug-build-only panel: jump the mock scenario directly and inspect the
 * backend events driving the UI. */
export function DebugPanel() {
  const stages: { value: MockStage; label: string }[] = [
    { value: "off", label: "Off" },
    { value: "champselect", label: "Champ Select" },
    { value: "ingame", label: "In Game" },
  ];

  return (
    <div class="flex flex-col gap-2 min-h-0">
      <div class="font-hx-serif text-[11px] font-bold tracking-[0.28em] text-hx-gold">DEBUG</div>
      <div class="flex flex-col gap-1">
        <span class="text-[11px] text-hx-muted">モックシナリオ</span>
        <div class="grid grid-cols-3 gap-1 rounded border border-hx-border bg-hx-bg-raised p-1">
          <For each={stages}>
            {(option) => (
              <button
                type="button"
                class={`rounded px-2 py-1 font-hx-serif text-[10px] font-semibold tracking-[0.16em] cursor-pointer ${
                  mockStage() === option.value
                    ? "bg-hx-gold-wash text-hx-gold"
                    : "bg-transparent text-hx-muted hover:text-hx-gold"
                }`}
                onClick={() => selectMockStage(option.value)}
              >
                {option.label}
              </button>
            )}
          </For>
        </div>
      </div>
      <button
        type="button"
        class="rounded border border-hx-border bg-hx-bg-raised px-2 py-1 font-hx-serif text-[10px] font-semibold tracking-[0.16em] text-hx-muted hover:text-hx-gold cursor-pointer"
        onClick={() => setPlaygroundOpen(true)}
      >
        UI Playground
      </button>
      <div class="flex items-center justify-between">
        <span class="text-[11px] text-hx-muted">イベントログ ({eventLog().length})</span>
        <button
          type="button"
          class="text-[10px] text-hx-muted hover:text-hx-gold cursor-pointer"
          onClick={clearEventLog}
        >
          クリア
        </button>
      </div>
      <div class="flex flex-col gap-1 overflow-y-auto min-h-0 max-h-48 rounded border border-hx-border bg-hx-bg-raised p-1 font-mono text-[10px]">
        <For each={eventLog()}>
          {(entry) => (
            <details class="text-hx-text">
              <summary class="cursor-pointer whitespace-nowrap overflow-hidden text-ellipsis">
                <span class="text-hx-muted">{entry.time}</span>{" "}
                <span class="text-hx-gold">{entry.event}</span>
              </summary>
              <pre class="whitespace-pre-wrap break-all text-hx-muted pl-2">
                {JSON.stringify(entry.payload, null, 2)}
              </pre>
            </details>
          )}
        </For>
      </div>
    </div>
  );
}
