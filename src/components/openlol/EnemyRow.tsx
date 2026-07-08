import { createMemo, Index, Show } from "solid-js";
import { assetsReady, champIconByKey, champName } from "../../assets";
import { champSelect } from "../../state/backend";
import { Icon } from "../Icon";

export function EnemyRow() {
  const ids = createMemo(() => {
    const cs = champSelect();
    return cs?.enemyChampionIds.length ? cs.enemyChampionIds : [0, 0, 0, 0, 0];
  });

  return (
    <div class="mb-3">
      <div class="text-[9px] font-bold tracking-[0.18em] text-hx-muted mb-1.5">ENEMY TEAM</div>
      <div class="flex gap-2">
        <Index each={ids()}>
          {(id) => (
            <div
              class={`w-10 h-10 flex items-center justify-center bg-hx-bg-raised border rounded-md overflow-hidden font-hx-display text-[15px] ${
                id() > 0 ? "border-hx-red text-hx-muted" : "border-hx-border text-hx-muted"
              }`}
              title={id() > 0 ? champName(id()) : undefined}
            >
              <Show when={id() > 0 && assetsReady()} fallback="?">
                <Icon url={champIconByKey(id())} class="w-full h-full object-cover" />
              </Show>
            </div>
          )}
        </Index>
      </div>
    </div>
  );
}
