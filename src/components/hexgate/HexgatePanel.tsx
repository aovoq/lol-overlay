import { createEffect, createMemo, onMount, Show } from "solid-js";
import { phaseChipLabel } from "../../lib/hexgate";
import { initWindowDrag, initWindowMoveSave } from "../../lib/drag";
import { champSelect, phase, windowMode } from "../../state/backend";
import {
  pinned as pinnedSetting,
  setGearHidden,
  togglePinned,
} from "../../state/settings";
import { BuildArea } from "./BuildArea";
import { Counters } from "./Counters";
import { EnemyRow } from "./EnemyRow";
import { ImportButton } from "./ImportButton";
import { Matchup } from "./Matchup";
import { RoleChips } from "./RoleChips";
import { StatsRow } from "./StatsRow";
import { Tabs } from "./Tabs";
import { TierLists } from "./TierLists";

export function HexgatePanel() {
  const show = createMemo(
    () =>
      windowMode() === "champselect" &&
      ((champSelect()?.active ?? false) || pinnedSetting()),
  );

  const phaseLabel = createMemo(() => {
    const p = phase();
    return p ? phaseChipLabel(p) : "CHAMP SELECT";
  });

  onMount(() => {
    initWindowMoveSave(() => windowMode());
  });

  createEffect(() => {
    document.body.classList.toggle("champselect", windowMode() === "champselect");
  });

  return (
    <Show when={show()}>
      <section
        class="hexgate hexgate-shell fixed inset-0 flex flex-col box-border pointer-events-auto bg-hx-bg border border-hx-border rounded-lg text-hx-text text-[13px] tabular-nums overflow-hidden"
        data-hit
      >
        <header
          ref={(el) => initWindowDrag(el)}
          class="flex-none h-12 flex items-center px-3.5 border-b border-hx-border relative cursor-grab active:cursor-grabbing"
        >
          <div class="flex items-center gap-2 text-hx-gold font-hx-serif font-bold text-[15px] tracking-[0.24em]">
            <svg
              viewBox="0 0 24 24"
              aria-hidden="true"
              class="w-[18px] h-[18px] fill-none stroke-current stroke-[1.6]"
            >
              <polygon points="12 2.5 20.2 7.25 20.2 16.75 12 21.5 3.8 16.75 3.8 7.25" />
            </svg>
            <span>HEXGATE</span>
          </div>
          <div class="absolute left-1/2 -translate-x-1/2 border border-hx-border rounded px-3.5 py-1 font-hx-serif font-semibold text-[10px] tracking-[0.18em] text-hx-text">
            {phaseLabel()}
          </div>
          <button
            class={`ml-auto w-7 h-7 flex items-center justify-center border rounded cursor-pointer ${
              pinnedSetting()
                ? "text-hx-gold border-hx-gold bg-hx-gold-wash"
                : "text-hx-muted border-hx-border bg-transparent"
            }`}
            title="Keep the panel open after champ select"
            onClick={() => togglePinned()}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" class="w-[15px] h-[15px] fill-current">
              <path d="M16 9V4h1c.55 0 1-.45 1-1s-.45-1-1-1H7c-.55 0-1 .45-1 1s.45 1 1 1h1v5c0 1.66-1.34 3-3 3v2h5.97v7l1 1 1-1v-7H19v-2c-1.66 0-3-1.34-3-3z" />
            </svg>
          </button>
          <button
            class="ml-2 w-7 h-7 flex items-center justify-center bg-transparent border border-hx-border rounded text-hx-muted cursor-pointer hover:text-hx-gold"
            title="設定"
            onClick={() => setGearHidden((h) => !h)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" class="w-[15px] h-[15px] fill-current">
              <path d="M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z" />
            </svg>
          </button>
        </header>

        <div class="flex-1 min-h-0 grid grid-cols-[360px_1fr]">
          <aside class="border-r border-hx-border p-3.5 flex flex-col gap-2 min-h-0">
            <RoleChips />
            <TierLists />
          </aside>

          <main class="p-4 px-[18px] pb-3.5 flex flex-col min-h-0">
            <EnemyRow />
            <Counters />
            <div class="border-t border-hx-border my-2.5" />
            <Matchup />
            <Tabs />
            <BuildArea />
            <StatsRow />
            <ImportButton />
          </main>
        </div>
      </section>
    </Show>
  );
}
