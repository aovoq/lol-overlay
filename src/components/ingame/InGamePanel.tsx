import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { assetsReady, champIconByName } from "../../assets";
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
import { RecommendedItems } from "./RecommendedItems";
import { SkillOrder } from "./SkillOrder";
import { ThreatChips } from "./ThreatChips";

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
    const cleanupDrag = props.embedded ? undefined : initPanelDrag(panelEl, headEl);

    const onTransitionEnd = (event: TransitionEvent) => {
      if (props.embedded || event.target !== panelEl || event.propertyName !== "width") return;
      clampPanelToViewport(panelEl);
      saveIngamePanelPosition(panelEl);
      reportHitRegions();
    };
    panelEl.addEventListener("transitionend", onTransitionEnd);
    onCleanup(() => {
      cleanupDrag?.();
      panelEl.removeEventListener("transitionend", onTransitionEnd);
    });
  });

  const toggleCollapse = () => {
    const next = !ingameCollapsed();
    setIngameCollapsed(next);
    if (!props.embedded) reportHitRegions();
  };

  const recs = () => recommendations();

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
        <span class="inline-flex items-center gap-[7px] text-hx-accent font-hx-display text-xs font-extrabold tracking-[0.32em] whitespace-nowrap">
          <svg
            viewBox="0 0 24 24"
            aria-hidden="true"
            class="w-[13px] h-[13px] fill-none stroke-hx-accent stroke-[1.6]"
          >
            <polygon points="12 2.5 20.2 7.25 20.2 16.75 12 21.5 3.8 16.75 3.8 7.25" />
          </svg>
          {APP_NAME}
        </span>
        <span class="inline-flex items-center gap-2">
          <span class="px-[9px] py-[3px] border border-hx-border rounded-[3px] font-hx-display text-[9px] font-semibold tracking-[0.22em] text-hx-text whitespace-nowrap">
            IN GAME
          </span>
          <button
            type="button"
            class="w-[22px] h-[22px] flex items-center justify-center p-0 bg-transparent border border-hx-border rounded text-hx-muted hover:text-hx-accent hover:border-hx-accent cursor-pointer"
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
                      class="w-[38px] h-[38px] rounded-[5px] border border-hx-accent-dim object-cover"
                    />
                  </Show>
                  <div class="flex flex-col gap-px min-w-0">
                    <span class="text-hx-text font-bold text-sm truncate">
                      {e().selfChampion || "—"}
                    </span>
                    <span class="text-hx-accent-dim font-hx-display text-[9px] font-semibold tracking-[0.26em]">
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
                  <ThreatChips threats={e().threats} />
                </div>

                <SkillOrder order={e().skillOrder} championImageId={e().selfRawName} />

                <div class="hx-section-title px-3 py-2 pb-1.5">RECOMMENDED BUILD</div>

                <ScrollArea
                  class="rec-list max-h-[52vh]"
                  contentClass="px-2 pb-2"
                  hit={!props.embedded}
                >
                  <RecommendedItems items={e().items} />
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
