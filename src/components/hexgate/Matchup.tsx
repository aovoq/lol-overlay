import { createMemo, Show } from "solid-js";
import { assetsReady, champIconByKey, champName } from "../../assets";
import { roleLabel } from "../../lib/hexgate";
import { champSelect, selectedRole, vsEnemyId } from "../../state/backend";
import { Icon } from "../Icon";

function effectiveRole() {
  const cs = champSelect();
  return cs?.myRole || selectedRole();
}

export function Matchup() {
  const my = createMemo(() => champSelect()?.myChampionId ?? 0);
  const enemy = createMemo(() => vsEnemyId());

  return (
    <Show when={my()}>
      <div class="flex items-center gap-1.5 text-[13px]">
        <span class="font-hx-serif font-semibold text-[11px] tracking-[0.16em] text-hx-muted">
          {roleLabel(effectiveRole())} ·{" "}
        </span>
        <Show when={assetsReady()}>
          <Icon
            url={champIconByKey(my())}
            class="w-5 h-5 rounded border border-hx-border object-cover"
          />
        </Show>
        <span class="text-hx-gold font-semibold">
          {champName(my()) || `#${my()}`}
        </span>
        <Show when={enemy()}>
          <span class="text-hx-muted italic">vs</span>
          <Show when={assetsReady()}>
            <Icon
              url={champIconByKey(enemy())}
              class="w-5 h-5 rounded border border-hx-border object-cover"
            />
          </Show>
          <span class="text-hx-text">
            {champName(enemy()) || `#${enemy()}`}
          </span>
        </Show>
      </div>
    </Show>
  );
}
