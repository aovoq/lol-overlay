import { createMemo, For, Show } from "solid-js";
import { assetsReady, champIconByKey, champName, fmtPct } from "../../assets";
import { counterCache } from "../../state/caches";
import type { CounterEntry } from "../../types";
import { Icon } from "../Icon";
import { SectionError } from "./SectionError";

export function Counters(props: {
  championId: number;
  role: string;
  /** Hover preview hook (draft board: peek at a counter pick's runes). */
  onHoverChampion?: (championId: number) => void;
}) {
  const enemy = createMemo(() => props.championId);
  const role = createMemo(() => props.role);
  const entry = createMemo(() => {
    const e = enemy();
    return e ? counterCache.get(`${e}|${role()}`) : null;
  });

  const counters = createMemo((): CounterEntry[] => {
    const e = entry();
    if (e?.state !== "ok") return [];
    return e.value.slice(0, 8);
  });
  const cacheKey = createMemo(() => {
    const e = enemy();
    return e ? `${e}|${role()}` : "";
  });
  const err = createMemo(() => {
    const e = entry();
    return e?.state === "err" ? e.error : "";
  });

  return (
    <Show when={enemy()}>
      <div>
        <div class="text-hx-muted text-xs mb-1.5">
          Counters for {champName(enemy()) || `#${enemy()}`}
        </div>
        <div class="flex gap-2.5 min-h-[46px] items-center">
          <Show
            when={entry()?.state === "loading"}
            fallback={
              <Show
                when={entry()?.state !== "err"}
                fallback={
                  <SectionError message={err()} onRetry={() => counterCache.refetch(cacheKey())} />
                }
              >
                <Show
                  when={counters().length > 0}
                  fallback={<span class="text-hx-muted">Not enough data yet</span>}
                >
                  <For each={counters()}>
                    {(c) => (
                      <div
                        class="w-[34px] text-center flex flex-col gap-0.5 items-center"
                        onMouseEnter={() => props.onHoverChampion?.(c.championId)}
                        onMouseLeave={() => props.onHoverChampion?.(0)}
                      >
                        <Show when={assetsReady()}>
                          <Icon
                            url={champIconByKey(c.championId)}
                            class="w-8 h-8 rounded border border-hx-border object-cover"
                            title={champName(c.championId)}
                          />
                        </Show>
                        <span
                          class={`text-[10px] ${c.winRate > 0.51 ? "text-hx-up" : "text-hx-muted"}`}
                        >
                          {fmtPct(c.winRate)}
                        </span>
                      </div>
                    )}
                  </For>
                </Show>
              </Show>
            }
          >
            <For each={Array.from({ length: 8 }, (_, i) => i)}>
              {() => <div class="hx-skel w-8 h-11 rounded" />}
            </For>
          </Show>
        </div>
      </div>
    </Show>
  );
}
