import { For, Show } from "solid-js";
import { assetsReady, itemIconUrl } from "../../assets";
import type { ItemRecommendation } from "../../types";
import { Icon } from "../Icon";

/** Ranked recommended-build list with per-item score bars. */
export function RecommendedItems(props: { items: ItemRecommendation[] }) {
  return (
    <ul class="list-none m-0 flex flex-col gap-[5px]">
      <For each={props.items}>
        {(it, i) => (
          <li
            class={`flex flex-row items-center gap-[9px] px-2 py-1.5 border rounded-[5px] bg-hx-bg-raised ${
              i() === 0 ? "border-hx-keystone-border" : "border-transparent"
            }`}
          >
            <span class="w-3.5 flex-none text-right text-[10px] font-extrabold text-hx-accent-dim tabular-nums">
              {i() + 1}
            </span>
            <Show when={assetsReady()}>
              <Icon
                url={itemIconUrl(it.itemId)}
                class="w-8 h-8 rounded border border-hx-border flex-none"
              />
            </Show>
            <div class="flex flex-col min-w-0 flex-1">
              <span class="font-semibold text-hx-text truncate">{it.name}</span>
              <span class="text-[10.5px] text-hx-muted">{it.reason}</span>
              <div
                class="h-0.5 mt-1 rounded-sm bg-gradient-to-r from-hx-accent to-hx-accent-dim opacity-85"
                style={{ width: `${Math.round(it.score * 100)}%` }}
              />
            </div>
          </li>
        )}
      </For>
    </ul>
  );
}
