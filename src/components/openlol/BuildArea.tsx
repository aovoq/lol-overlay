import { createMemo, For, Show } from "solid-js";
import { champName, getPerk, getShard, getStyle } from "../../assets";
import { OPENLOL_MARK_SVG, roleLabel } from "../../lib/openlol";
import { buildCache, buildKey } from "../../state/caches";
import type { RuneBuild } from "../../types";
import { Icon } from "../Icon";
import { ScrollArea } from "../ScrollArea";
import { SectionError } from "./SectionError";

function TreeHead(props: { styleId: number; primary: boolean }) {
  const s = () => getStyle(props.styleId);
  return (
    <div
      class="flex items-center gap-2 my-2 mx-0 font-hx-display font-semibold text-xs tracking-[0.16em] text-hx-accent"
      style={!props.primary && s() ? { color: s()?.color ?? "" } : undefined}
    >
      <Show when={s()?.icon}>
        <Icon url={s()?.icon ?? ""} class="w-4 h-4" />
      </Show>
      {(s()?.name ?? `Style ${props.styleId}`).toUpperCase()}
    </div>
  );
}

function KeystoneCard(props: { perkId: number }) {
  const perk = () => getPerk(props.perkId);
  return (
    <div class="flex items-center gap-3 px-3 py-2 bg-hx-bg-raised border border-hx-keystone-border rounded-md">
      <Icon url={perk()?.icon ?? ""} class="w-11 h-11" />
      <span class="text-[15px] font-bold text-hx-text">{perk()?.name ?? `#${props.perkId}`}</span>
    </div>
  );
}

function RuneRow(props: { perkId: number }) {
  const perk = () => getPerk(props.perkId);
  return (
    <div class="flex items-center gap-2.5 px-2 py-1 text-[13px] text-hx-text">
      <Icon url={perk()?.icon ?? ""} class="w-7 h-7" />
      {perk()?.name ?? `#${props.perkId}`}
    </div>
  );
}

function ShardChip(props: { shardId: number }) {
  const info = () => getShard(props.shardId);
  return (
    <div class="inline-flex items-center gap-1.5 border border-hx-border rounded-full px-2.5 py-1 text-[11px] text-hx-text">
      <Show when={info().icon}>
        <Icon url={info().icon} class="w-4 h-4 opacity-90" />
      </Show>
      {info().label}
    </div>
  );
}

function RunePage(props: { build: RuneBuild }) {
  const b = () => props.build;
  const keystone = () => b().primaryPerkIds[0];
  const minors = () => b().primaryPerkIds.slice(1);

  return (
    <div class="flex flex-col gap-1.5">
      <TreeHead styleId={b().primaryStyleId} primary={true} />
      <Show when={keystone()}>{(id) => <KeystoneCard perkId={id()} />}</Show>
      <For each={minors()}>{(id) => <RuneRow perkId={id} />}</For>

      <TreeHead styleId={b().subStyleId} primary={false} />
      <For each={b().subPerkIds}>{(id) => <RuneRow perkId={id} />}</For>

      <span class="block mt-2 text-hx-muted font-hx-display font-semibold text-[11px] tracking-[0.16em]">
        SHARDS
      </span>
      <div class="flex flex-wrap gap-2">
        <For each={b().shardIds}>{(id) => <ShardChip shardId={id} />}</For>
      </div>
    </div>
  );
}

function BuildSkeleton() {
  return (
    <div class="flex flex-col gap-1.5">
      <div class="hx-skel h-[62px] rounded-md" />
      <For each={[0, 1, 2, 3, 4]}>{() => <div class="hx-skel h-9 rounded-md" />}</For>
    </div>
  );
}

function BigEmpty(props: { role: string }) {
  return (
    <div class="flex flex-col items-center gap-2.5 mt-14 mx-auto max-w-[340px] text-center text-hx-muted">
      <div
        class="text-hx-accent-dim w-11 h-11 [&_svg]:w-full [&_svg]:h-full"
        innerHTML={OPENLOL_MARK_SVG}
      />
      <div class="font-hx-display font-bold text-sm tracking-[0.2em] text-hx-accent">
        {roleLabel(props.role)}
      </div>
      <div class="text-xs leading-normal">Hover a champion to see runes</div>
    </div>
  );
}

function NotEnoughData(props: { championId: number; matchup: boolean }) {
  return (
    <div class="flex flex-col items-center gap-2.5 mt-14 mx-auto max-w-[340px] text-center text-hx-muted">
      <div
        class="text-hx-accent-dim w-11 h-11 [&_svg]:w-full [&_svg]:h-full"
        innerHTML={OPENLOL_MARK_SVG}
      />
      <div class="text-base text-hx-text">Not enough data</div>
      <Show when={props.matchup}>
        <div class="text-xs leading-normal">
          Too few games for {champName(props.championId) || "this champion"} in this matchup to
          provide reliable rune recommendations.
        </div>
      </Show>
    </div>
  );
}

export function BuildArea(props: { championId: number; role: string; enemyId?: number | null }) {
  const role = createMemo(() => props.role);
  const target = createMemo(() =>
    props.championId > 0 ? { champ: props.championId, enemy: props.enemyId ?? null } : null,
  );
  const cacheKey = createMemo(() => {
    const t = target();
    return t ? buildKey(t.champ, role(), t.enemy) : "";
  });
  const entry = createMemo(() => {
    const key = cacheKey();
    return key ? buildCache.get(key) : null;
  });

  const build = createMemo(() => {
    const e = entry();
    return e?.state === "ok" ? e.value : null;
  });

  const err = createMemo(() => {
    const e = entry();
    return e?.state === "err" ? e.error : "";
  });

  const loading = createMemo(() => entry()?.state === "loading");

  return (
    <ScrollArea class="flex-1 min-h-0" contentClass="pt-3 pb-1.5 pr-1 pl-0">
      <Show when={target()} fallback={<BigEmpty role={role()} />}>
        {(t) => (
          <Show when={!loading()} fallback={<BuildSkeleton />}>
            <Show
              when={build()}
              fallback={
                <Show
                  when={err() === "not-enough-data"}
                  fallback={
                    <SectionError message={err()} onRetry={() => buildCache.refetch(cacheKey())} />
                  }
                >
                  <NotEnoughData championId={t().champ} matchup={t().enemy !== null} />
                </Show>
              }
            >
              {(b) => <RunePage build={b()} />}
            </Show>
          </Show>
        )}
      </Show>
    </ScrollArea>
  );
}
