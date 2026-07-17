import { createMemo, For, Show } from "solid-js";
import { assetsReady, champIconByKey, champName, fmtCompact, fmtPct } from "../../assets";
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
      <div class="min-w-0">
        <div class="flex items-end justify-between gap-2 mb-1.5">
          <span class="min-w-0 text-hx-muted text-xs truncate">
            Counters for {champName(enemy()) || `#${enemy()}`}
          </span>
          <span class="shrink-0 text-[8px] font-bold tracking-[0.08em] text-hx-muted">
            WR · GAMES
          </span>
        </div>
        <div class="flex flex-wrap gap-x-2.5 gap-y-2 min-h-[46px] items-start">
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
                        class="w-[34px] shrink-0 text-center flex flex-col gap-0.5 items-center"
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
                        <span class="flex flex-col items-center leading-tight">
                          <span
                            class={`text-[10px] ${c.winRate > 0.51 ? "text-hx-up" : "text-hx-muted"}`}
                          >
                            {fmtPct(c.winRate)}
                          </span>
                          <span
                            class="text-[8px] text-hx-muted"
                            title={`${c.games.toLocaleString()} games`}
                          >
                            {fmtCompact(c.games)}
                          </span>
                        </span>
                      </div>
                    )}
                  </For>
                </Show>
              </Show>
            }
          >
            <For each={Array.from({ length: 8 }, (_, i) => i)}>
              {() => <div class="hx-skel w-8 h-11 shrink-0 rounded" />}
            </For>
          </Show>
        </div>
      </div>
    </Show>
  );
}
