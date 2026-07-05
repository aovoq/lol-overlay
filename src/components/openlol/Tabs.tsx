import { createEffect, createMemo, For, onCleanup, onMount, Show } from "solid-js";
import { assetsReady, champIconByKey, champName } from "../../assets";
import {
  activeTab,
  champSelect,
  setActiveTab,
  setUserPickedVsEnemy,
  setVsEnemyId,
  setVsMenuOpen,
  vsEnemyId,
  vsMenuOpen,
} from "../../state/backend";
import { Icon } from "../Icon";

function revealedEnemies() {
  const cs = champSelect();
  return (cs?.enemyChampionIds ?? []).filter((id) => id > 0);
}

export function Tabs() {
  const enemy = createMemo(() => vsEnemyId());

  createEffect(() => {
    if (!enemy()) setVsMenuOpen(false);
  });

  onMount(() => {
    const onDocClick = (ev: MouseEvent) => {
      const t = ev.target as HTMLElement;
      if (!t.closest(".hx-vs-menu") && !t.closest(".hx-tab-vs")) {
        setVsMenuOpen(false);
      }
    };
    document.addEventListener("click", onDocClick);
    onCleanup(() => document.removeEventListener("click", onDocClick));
  });

  const toggleVsMenu = () => setVsMenuOpen(!vsMenuOpen());

  return (
    <nav class="relative flex gap-[26px] border-b border-hx-border mt-1.5">
      <button
        type="button"
        class={`inline-flex items-center gap-1.5 bg-none border-b-2 -mb-px py-2.5 px-0.5 font-hx-serif font-semibold text-xs tracking-[0.12em] cursor-pointer ${
          activeTab() === "best"
            ? "text-hx-gold border-b-hx-gold"
            : "text-hx-muted border-b-transparent"
        }`}
        onClick={() => setActiveTab("best")}
      >
        BEST BUILD
      </button>

      <div class="relative inline-flex">
        <button
          type="button"
          class={`hx-tab-vs inline-flex items-center gap-1.5 bg-none border-b-2 -mb-px py-2.5 px-0.5 font-hx-serif font-semibold text-xs tracking-[0.12em] ${
            activeTab() === "vs"
              ? "text-hx-gold border-b-hx-gold"
              : "text-hx-muted border-b-transparent"
          } ${!enemy() ? "opacity-45 cursor-default" : "cursor-pointer"}`}
          disabled={!enemy()}
          onClick={(ev) => {
            if (!enemy()) return;
            if ((ev.target as HTMLElement).closest(".hx-chevron")) {
              toggleVsMenu();
              return;
            }
            setActiveTab("vs");
          }}
        >
          <Show when={enemy()} fallback="VS ...">
            <Show when={assetsReady()}>
              <Icon
                url={champIconByKey(enemy())}
                class="w-[18px] h-[18px] rounded-[3px] border border-hx-border object-cover"
              />
            </Show>
            {` VS ${(champName(enemy()) || `#${enemy()}`).toUpperCase()} `}
            <span class="hx-chevron pl-0.5">▾</span>
          </Show>
        </button>

        <Show when={vsMenuOpen() && enemy()}>
          <div class="hx-vs-menu hx-menu absolute top-[calc(100%+4px)] left-0 z-10 min-w-[180px] flex flex-col bg-hx-bg-raised border border-hx-border rounded-md p-1">
            <For each={revealedEnemies()}>
              {(id) => (
                <button
                  type="button"
                  class="flex items-center gap-2 w-full text-left bg-none border-none rounded px-2.5 py-1.5 text-hx-text text-[13px] cursor-pointer hover:bg-hx-bg hover:text-hx-gold"
                  onClick={() => {
                    setVsEnemyId(id);
                    setUserPickedVsEnemy(true);
                    setActiveTab("vs");
                    setVsMenuOpen(false);
                  }}
                >
                  <Show when={assetsReady()}>
                    <Icon
                      url={champIconByKey(id)}
                      class="w-[22px] h-[22px] rounded-[3px] border border-hx-border object-cover"
                    />
                  </Show>
                  {champName(id) || `#${id}`}
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
    </nav>
  );
}
