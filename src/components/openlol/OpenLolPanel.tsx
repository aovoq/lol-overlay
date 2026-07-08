import { createEffect, createMemo, onCleanup, onMount, Show } from "solid-js";
import { initWindowDrag } from "../../lib/drag";
import { APP_NAME, phaseChipLabel } from "../../lib/openlol";
import {
  champSelect,
  phase,
  selectedRole,
  setUserPickedVsEnemy,
  setVsEnemyId,
  userPickedVsEnemy,
  vsEnemyId,
} from "../../state/backend";
import { tierCache } from "../../state/caches";
import { BuildArea } from "./BuildArea";
import { Counters } from "./Counters";
import { EnemyRow } from "./EnemyRow";
import { ImportButton } from "./ImportButton";
import { Matchup } from "./Matchup";
import { RoleChips } from "./RoleChips";
import { StatsRow } from "./StatsRow";
import { Tabs } from "./Tabs";
import { TierLists } from "./TierLists";

function effectiveRole() {
  const cs = champSelect();
  return cs?.myRole || selectedRole();
}

export function OpenLolPanel() {
  const show = createMemo(() => champSelect()?.active ?? false);
  const revealedEnemies = createMemo(() => {
    const cs = champSelect();
    return (cs?.enemyChampionIds ?? []).filter((id) => id > 0);
  });
  const likelyLaneEnemy = createMemo(() => {
    const revealed = revealedEnemies();
    if (!revealed.length) return 0;

    const entry = tierCache.get(effectiveRole());
    if (entry.state !== "ok") return revealed[0];

    const pickRates = new Map(entry.value.map((t) => [t.championId, t.pickRate]));
    return (
      revealed
        .map((id) => ({ id, pickRate: pickRates.get(id) ?? -1 }))
        .sort((a, b) => b.pickRate - a.pickRate)[0]?.id ?? revealed[0]
    );
  });

  const phaseLabel = createMemo(() => {
    const p = phase();
    return p ? phaseChipLabel(p) : "CHAMP SELECT";
  });

  onMount(() => {
    const header = document.querySelector<HTMLElement>(".openlol-header");
    const cleanup = initWindowDrag(header ?? undefined);
    onCleanup(cleanup);
  });

  createEffect(() => {
    const revealed = revealedEnemies();
    if (!revealed.length) {
      setVsEnemyId(0);
      setUserPickedVsEnemy(false);
      return;
    }

    if (!revealed.includes(vsEnemyId()) || !userPickedVsEnemy()) {
      setVsEnemyId(likelyLaneEnemy());
      setUserPickedVsEnemy(false);
    }
  });

  return (
    <Show when={show()}>
      <section
        class="openlol openlol-shell fixed inset-0 flex flex-col box-border pointer-events-auto bg-hx-bg border border-hx-border rounded text-hx-text text-[13px] tabular-nums overflow-hidden"
        data-hit
      >
        <header class="openlol-header flex-none h-12 flex items-center px-3.5 border-b border-hx-border relative cursor-grab active:cursor-grabbing">
          <div class="flex items-center gap-2 text-hx-accent font-hx-display font-extrabold text-[15px] tracking-[0.24em]">
            <svg
              viewBox="0 0 24 24"
              aria-hidden="true"
              class="w-[18px] h-[18px] fill-none stroke-current stroke-[1.6]"
            >
              <polygon points="12 2.5 20.2 7.25 20.2 16.75 12 21.5 3.8 16.75 3.8 7.25" />
            </svg>
            <span>{APP_NAME}</span>
          </div>
          <div class="absolute left-1/2 -translate-x-1/2 border border-hx-border rounded px-3.5 py-1 font-hx-display font-semibold text-[10px] tracking-[0.18em] text-hx-text">
            {phaseLabel()}
          </div>
        </header>

        <div class="flex-1 min-h-0 grid grid-cols-[360px_1fr]">
          <aside class="border-r border-hx-border p-4 flex flex-col gap-2.5 min-h-0">
            <RoleChips />
            <TierLists />
          </aside>

          <main class="p-5 pb-4 flex flex-col min-h-0">
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
