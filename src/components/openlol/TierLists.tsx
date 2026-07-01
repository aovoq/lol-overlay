import { createEffect, createMemo, For, type JSX, Show } from "solid-js";
import { assetsReady, champIconByKey, champName, fmtCompact, fmtPct } from "../../assets";
import { roleLabel } from "../../lib/openlol";
import { champSelect, selectedRole, setHoverChampId } from "../../state/backend";
import { tierCache } from "../../state/caches";
import type { TierEntry } from "../../types";
import { Icon } from "../Icon";
import { SectionError } from "./SectionError";

function effectiveRole() {
  const cs = champSelect();
  return cs?.myRole || selectedRole();
}

function ChampRow(props: { championId: number; children?: JSX.Element }) {
  return (
    <div
      class="flex-none flex items-center gap-2 px-2 py-1 rounded-md hover:bg-hx-bg-raised"
      onMouseEnter={() => setHoverChampId(props.championId)}
      onMouseLeave={() => setHoverChampId(0)}
    >
      <Show when={assetsReady()}>
        <Icon
          url={champIconByKey(props.championId)}
          class="w-7 h-7 rounded border border-hx-border object-cover"
        />
      </Show>
      <span class="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
        {champName(props.championId) || `#${props.championId}`}
      </span>
      {props.children}
    </div>
  );
}

function StrongRow(props: { entry: TierEntry }) {
  const t = () => props.entry;
  const delta = () => {
    const d = t().winRateDelta;
    if (Math.abs(d) < 0.5) return null;
    const up = d > 0;
    return `${up ? "▲" : "▼"}${Math.abs(d).toFixed(1)}`;
  };

  return (
    <ChampRow championId={t().championId}>
      <span class="w-12 text-right font-bold text-hx-text">{fmtPct(t().winRate)}</span>
      <span
        class={`w-[38px] text-right text-[11px] ${
          delta() ? (t().winRateDelta > 0 ? "text-hx-up" : "text-hx-red") : ""
        }`}
      >
        {delta()}
      </span>
      <span class="w-11 text-right text-xs text-hx-muted">
        {t().games > 0 ? fmtCompact(t().games) : fmtPct(t().pickRate)}
      </span>
    </ChampRow>
  );
}

function BanRow(props: { entry: TierEntry }) {
  const t = () => props.entry;
  return (
    <ChampRow championId={t().championId}>
      <span class="w-12 text-right font-bold text-hx-red">{fmtPct(t().winRate)}</span>
      <span class="w-11 text-right text-xs text-hx-muted">{fmtPct(t().pickRate)}</span>
    </ChampRow>
  );
}

function SkeletonRows(props: { count: number }) {
  return (
    <For each={Array.from({ length: props.count }, (_, i) => i)}>
      {() => <div class="flex-none hx-skel h-7 rounded-md" />}
    </For>
  );
}

export function TierLists() {
  const role = createMemo(() => effectiveRole());
  const bannedKey = createMemo(() => {
    const cs = champSelect();
    return [...new Set([...(cs?.myBans ?? []), ...(cs?.enemyBans ?? [])].filter((id) => id > 0))]
      .sort((a, b) => a - b)
      .join(",");
  });
  const entry = createMemo(() => tierCache.get(role()));

  // Clear hover when list inputs change
  createEffect(() => {
    role();
    entry().state;
    bannedKey();
    assetsReady();
    setHoverChampId(0);
  });

  createEffect(() => {
    if (!champSelect()?.active) setHoverChampId(0);
  });

  const strong = createMemo(() => {
    const e = entry();
    if (e.state !== "ok") return [];
    return e.value.filter((t) => t.pickRate >= 0.005).sort((a, b) => b.winRate - a.winRate);
  });

  const bans = createMemo(() => {
    const e = entry();
    if (e.state !== "ok") return [];
    const banSet = new Set(bannedKey().split(",").filter(Boolean).map(Number));
    return e.value
      .filter((t) => !banSet.has(t.championId))
      .sort((a, b) => (b.winRate - 0.5) * b.pickRate - (a.winRate - 0.5) * a.pickRate)
      .slice(0, 10);
  });

  const errMsg = createMemo(() => {
    const e = entry();
    return e.state === "err" ? e.error : "";
  });

  const isLoading = createMemo(() => entry().state === "loading");
  const isOk = createMemo(() => entry().state === "ok");

  return (
    <>
      <div class="flex flex-col gap-0.5 font-hx-serif font-semibold text-[11px] tracking-[0.16em] text-hx-gold-dim px-0.5 pt-1 pb-0.5">
        <span>{roleLabel(role())}</span>
        <span>STRONG PICKS</span>
      </div>
      <div class="min-h-0 overflow-y-auto flex flex-col gap-0.5 pr-1 flex-[1.3_1_0] hx-scroll">
        <Show
          when={isLoading()}
          fallback={
            <Show
              when={isOk()}
              fallback={
                <SectionError message={errMsg()} onRetry={() => tierCache.refetch(role())} />
              }
            >
              <For each={strong()}>{(t) => <StrongRow entry={t} />}</For>
            </Show>
          }
        >
          <SkeletonRows count={8} />
        </Show>
      </div>

      <div class="flex flex-col gap-0.5 font-hx-serif font-semibold text-[11px] tracking-[0.16em] text-hx-gold-dim px-0.5 pt-1 pb-0.5">
        <span>BAN TARGETS</span>
      </div>
      <div class="min-h-0 overflow-y-auto flex flex-col gap-0.5 pr-1 flex-1 hx-scroll">
        <Show
          when={isLoading()}
          fallback={
            <Show
              when={isOk()}
              fallback={
                <SectionError message={errMsg()} onRetry={() => tierCache.refetch(role())} />
              }
            >
              <For each={bans()}>{(t) => <BanRow entry={t} />}</For>
            </Show>
          }
        >
          <SkeletonRows count={4} />
        </Show>
      </div>
    </>
  );
}
