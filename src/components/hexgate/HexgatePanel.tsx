import { createMemo, onMount, Show } from "solid-js";
import { initWindowDrag } from "../../lib/drag";
import { phaseChipLabel } from "../../lib/hexgate";
import { champSelect, phase } from "../../state/backend";
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
  const show = createMemo(() => champSelect()?.active ?? false);

  const phaseLabel = createMemo(() => {
    const p = phase();
    return p ? phaseChipLabel(p) : "CHAMP SELECT";
  });

  onMount(() => {
    const header = document.querySelector<HTMLElement>(".hexgate-header");
    initWindowDrag(header ?? undefined);
  });

  return (
    <Show when={show()}>
      <section
        class="hexgate hexgate-shell fixed inset-0 flex flex-col box-border pointer-events-auto bg-hx-bg border border-hx-border rounded-lg text-hx-text text-[13px] tabular-nums overflow-hidden"
        data-hit
      >
        <header class="hexgate-header flex-none h-12 flex items-center px-3.5 border-b border-hx-border relative cursor-grab active:cursor-grabbing">
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
