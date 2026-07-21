import { Index, Show } from "solid-js";
import { assetsReady, itemIconUrl } from "../../assets";
import type { ItemRecommendation } from "../../types";
import { Icon } from "../Icon";

/** Ranked build order as a compact icon path: item → item → item. */
export function ItemPath(props: { items: ItemRecommendation[]; max?: number }) {
  const items = () => props.items.slice(0, props.max ?? 6);

  return (
    <Show when={items().length > 0}>
      <div class="item-path">
        <Index each={items()}>
          {(item, i) => (
            <>
              <Show when={i > 0}>
                <span class="item-path-arrow" />
              </Show>
              <span class="item-path-slot">
                <Show when={assetsReady()}>
                  <Icon
                    url={itemIconUrl(item().itemId)}
                    title={`${item().name} · ${item().reason}`}
                    class="item-path-icon"
                  />
                </Show>
              </span>
            </>
          )}
        </Index>
      </div>
    </Show>
  );
}
