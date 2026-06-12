import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { assetsReady, champIconByKey } from "../assets";
import { runeImported } from "../state/backend";
import { Icon } from "./Icon";

export function RuneBanner() {
  const [visible, setVisible] = createSignal(false);
  const [pageName, setPageName] = createSignal("");
  const [champId, setChampId] = createSignal(0);
  let timer: number | undefined;

  createEffect(() => {
    const e = runeImported();
    if (!e) return;
    setPageName(e.pageName);
    setChampId(e.championId);
    setVisible(true);
    if (timer) window.clearTimeout(timer);
    timer = window.setTimeout(() => setVisible(false), 6000);
  });

  onCleanup(() => {
    if (timer) window.clearTimeout(timer);
  });

  return (
    <Show when={visible()}>
      <div class="panel fixed top-[18px] left-1/2 -translate-x-1/2 flex gap-2.5 items-center border-hx-gold-dim">
        <Show when={assetsReady()}>
          <Icon
            url={champIconByKey(champId())}
            class="w-[34px] h-[34px] rounded-md border border-hx-gold-dim object-cover"
          />
        </Show>
        <div class="flex flex-col leading-snug">
          <strong class="text-hx-gold font-hx-serif text-[11px] font-bold tracking-[0.18em] uppercase">
            Runes imported
          </strong>
          <span class="text-hx-muted text-xs">{pageName()}</span>
        </div>
      </div>
    </Show>
  );
}
