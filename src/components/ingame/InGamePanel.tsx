import { createEffect, createSignal, For, onMount, Show } from "solid-js";
import { assetsReady, champIconByName, itemIconUrl } from "../../assets";
import {
  applyPanelPosition,
  clampPanelToViewport,
  initPanelDrag,
  saveIngamePanelPosition,
} from "../../lib/drag";
import { reportHitRegions } from "../../lib/hitRegions";
import { APP_NAME } from "../../lib/openlol";
import { phase, recommendations } from "../../state/backend";
import { ingameCollapsed, ingamePos, setIngameCollapsed } from "../../state/layout";
import { Icon } from "../Icon";
import { ScrollArea } from "../ScrollArea";
import { SkillOrder } from "./SkillOrder";

function PanelBody(props: { embedded?: boolean }) {
  let panelEl!: HTMLDivElement;
  let headEl!: HTMLElement;

  createEffect(() => {
    if (props.embedded) return;
    const pos = ingamePos();
    if (pos) applyPanelPosition(panelEl, pos.left, pos.top);
  });

  createEffect(() => {
    recommendations();
    if (props.embedded) return;
    requestAnimationFrame(() => {
      if (panelEl) clampPanelToViewport(panelEl);
    });
  });

  onMount(() => {
    if (!props.embedded) initPanelDrag(panelEl, headEl);

    panelEl.addEventListener("transitionend", (event) => {
      if (props.embedded || event.target !== panelEl || event.propertyName !== "width") return;
      clampPanelToViewport(panelEl);
      saveIngamePanelPosition(panelEl);
      reportHitRegions();
    });
  });

  const toggleCollapse = () => {
    const next = !ingameCollapsed();
    setIngameCollapsed(next);
    if (!props.embedded) reportHitRegions();
  };

  const recs = () => recommendations();
  const threats = () => {
    const t = recs()?.threats;
    if (!t) return [];
    const chips: { kind: string; count?: number; label: string; cc?: boolean }[] = [
      { kind: "ad", count: t.adCount, label: "AD" },
      { kind: "ap", count: t.apCount, label: "AP" },
      { kind: "tank", count: t.tankCount, label: "TANK" },
    ];
    if (t.ccHeavy) chips.push({ kind: "cc", label: "CC HEAVY", cc: true });
    return chips;
  };

  const threatColor = (kind: string) => {
    switch (kind) {
      case "ad":
        return "text-hx-physical";
      case "ap":
        return "text-hx-magic";
      case "tank":
        return "text-hx-durable";
      default:
        return "";
    }
  };

  return (
    <div
      ref={panelEl}
      class={`ingame-panel panel p-0 overflow-hidden pointer-events-auto ${
        props.embedded
          ? "relative w-full"
          : "fixed top-20 right-4 transition-[width] duration-200 ease-[cubic-bezier(0.2,0,0,1)]"
      } ${props.embedded ? "" : ingameCollapsed() ? "collapsed w-[280px]" : "w-[448px]"}`}
    >
      <header
        ref={headEl}
        class={`ig-head flex justify-between items-center px-3 py-[9px] border-b transition-[border-color] duration-200 ${
          props.embedded ? "cursor-default" : "cursor-grab active:cursor-grabbing"
        } ${ingameCollapsed() ? "border-b-transparent cursor-default" : "border-b-hx-border"}`}
        data-hit={!props.embedded ? true : undefined}
      >
        <span class="inline-flex items-center gap-[7px] text-hx-gold font-hx-serif text-xs font-bold tracking-[0.32em] whitespace-nowrap">
          <svg
            viewBox="0 0 24 24"
            aria-hidden="true"
            class="w-[13px] h-[13px] fill-none stroke-hx-gold stroke-[1.6]"
          >
            <polygon points="12 2.5 20.2 7.25 20.2 16.75 12 21.5 3.8 16.75 3.8 7.25" />
          </svg>
          {APP_NAME}
        </span>
        <span class="inline-flex items-center gap-2">
          <span class="px-[9px] py-[3px] border border-hx-border rounded-[3px] font-hx-serif text-[9px] font-semibold tracking-[0.22em] text-hx-text whitespace-nowrap">
            IN GAME
          </span>
          <button
            type="button"
            class="w-[22px] h-[22px] flex items-center justify-center p-0 bg-transparent border border-hx-border rounded text-hx-muted hover:text-hx-gold hover:border-hx-gold cursor-pointer"
            title="折りたたみ切替"
            onClick={toggleCollapse}
          >
            <svg
              viewBox="0 0 24 24"
              aria-hidden="true"
              class={`w-3.5 h-3.5 fill-none stroke-current stroke-2 transition-transform duration-150 ${
                ingameCollapsed() ? "-rotate-90" : ""
              }`}
            >
              <path d="M7 10l5 5 5-5" />
            </svg>
          </button>
        </span>
      </header>

      <div
        class={`ig-collapse-wrap grid transition-[grid-template-rows] duration-200 ease-[cubic-bezier(0.2,0,0,1)] ${
          ingameCollapsed() ? "grid-rows-[0fr]" : "grid-rows-[1fr]"
        }`}
      >
        <div class="min-h-0 overflow-hidden">
          <Show when={recs()}>
            {(e) => (
              <>
                <div class="flex items-center gap-2.5 px-3 pt-2.5 pb-2">
                  <Show when={assetsReady()}>
                    <Icon
                      url={champIconByName(e().selfRawName)}
                      class="w-[38px] h-[38px] rounded-[5px] border border-hx-gold-dim object-cover"
                    />
                  </Show>
                  <div class="flex flex-col gap-px min-w-0">
                    <span class="text-hx-text font-bold text-sm truncate">
                      {e().selfChampion || "—"}
                    </span>
                    <span class="text-hx-gold-dim font-hx-serif text-[9px] font-semibold tracking-[0.26em]">
                      {e().selfPosition || ""}
                    </span>
                  </div>
                </div>

                <div class="flex flex-col gap-[7px] px-3 pb-2.5 border-b border-hx-border">
                  <div class="flex gap-1">
                    <Show when={assetsReady()}>
                      <For each={e().enemies}>
                        {(en) => (
                          <Icon
                            url={champIconByName(en.rawName)}
                            class="w-[26px] h-[26px] rounded border border-hx-red-soft object-cover"
                            title={en.name}
                          />
                        )}
                      </For>
                    </Show>
                  </div>
                  <div class="flex flex-wrap gap-1">
                    <For each={threats()}>
                      {(chip) => (
                        <span
                          class={`px-[7px] py-0.5 border rounded-[3px] bg-hx-bg-raised text-[10px] font-semibold tracking-[0.06em] ${
                            chip.cc
                              ? "text-hx-red border-hx-red-soft"
                              : "text-hx-muted border-hx-border"
                          }`}
                        >
                          <Show when={chip.count !== undefined} fallback={chip.label}>
                            <b class={`font-bold ${threatColor(chip.kind)}`}>{chip.count}</b>
                            {` ${chip.label}`}
                          </Show>
                        </span>
                      )}
                    </For>
                  </div>
                </div>

                <SkillOrder order={e().skillOrder} championImageId={e().selfRawName} />

                <div class="px-3 py-2 pb-1.5 font-hx-serif text-[10px] font-semibold tracking-[0.24em] text-hx-gold-dim">
                  RECOMMENDED BUILD
                </div>

                <ScrollArea class="rec-list max-h-[52vh]" contentClass="px-2 pb-2">
                  <ul class="list-none m-0 flex flex-col gap-[5px]">
                    <For each={e().items}>
                      {(it, i) => (
                        <li
                          class={`flex flex-row items-center gap-[9px] px-2 py-1.5 border rounded-[5px] bg-hx-bg-raised ${
                            i() === 0 ? "border-hx-keystone-border" : "border-transparent"
                          }`}
                        >
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
                              class="h-0.5 mt-1 rounded-sm bg-gradient-to-r from-hx-gold to-hx-gold-dim opacity-85"
                              style={{ width: `${Math.round(it.score * 100)}%` }}
                            />
                          </div>
                        </li>
                      )}
                    </For>
                  </ul>
                </ScrollArea>
              </>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
}

export function InGamePanel(props: { embedded?: boolean }) {
  const [visible, setVisible] = createSignal(false);
  let wasInGame = false;

  createEffect(() => {
    const p = phase();
    if (!p) return;
    if (wasInGame && !p.inGame) setVisible(false);
    wasInGame = p.inGame;
  });

  createEffect(() => {
    if (recommendations()) setVisible(true);
  });

  return (
    <Show when={visible()}>
      <PanelBody embedded={props.embedded} />
    </Show>
  );
}
