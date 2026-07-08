import { createMemo, Index, Show } from "solid-js";
import { assetsReady, champIconByKey, champName } from "../../assets";
import { champSelect, setUserPickedVsEnemy, setVsEnemyId, vsEnemyId } from "../../state/backend";
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
            <button
              type="button"
              class={`w-10 h-10 flex items-center justify-center bg-hx-bg-raised border rounded-md overflow-hidden font-hx-display text-[15px] ${
                id() > 0 ? "cursor-pointer text-hx-muted" : "cursor-default text-hx-muted"
              } ${
                id() > 0 && id() === vsEnemyId()
                  ? "border-hx-accent ring-1 ring-hx-accent"
                  : id() > 0
                    ? "border-hx-red"
                    : "border-hx-border"
              }`}
              disabled={id() <= 0}
              title={id() > 0 ? champName(id()) : undefined}
              onClick={() => {
                if (id() <= 0) return;
                setVsEnemyId(id());
                setUserPickedVsEnemy(true);
              }}
            >
              <Show when={id() > 0 && assetsReady()} fallback="?">
                <Icon url={champIconByKey(id())} class="w-full h-full object-cover" />
              </Show>
            </button>
          )}
        </Index>
      </div>
    </div>
  );
}
