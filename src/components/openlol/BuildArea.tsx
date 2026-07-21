import { createMemo, For, Show } from "solid-js";
import {
  champName,
  fmtPct,
  fmtThousands,
  getPerk,
  getShard,
  getStyle,
  getStyleTree,
  SHARD_ROWS,
} from "../../assets";
import { OPENLOL_MARK_SVG, roleLabel } from "../../lib/openlol";
import { buildCache, buildKey } from "../../state/caches";
import type { RuneBuild } from "../../types";
import { Icon } from "../Icon";
import { ScrollArea } from "../ScrollArea";
import { SectionError } from "./SectionError";

/** One rune-tree icon: full color + accent ring when picked, dimmed otherwise. */
function RuneCell(props: { perkId: number; picked: boolean; keystone?: boolean }) {
  const perk = () => getPerk(props.perkId);
  return (
    <Icon
      url={perk()?.icon ?? ""}
      title={perk()?.name ?? `#${props.perkId}`}
      class={`rune-cell ${props.keystone ? "rune-cell--keystone" : ""} ${
        props.picked ? (props.keystone ? "is-keystone-pick" : "is-picked") : ""
      }`}
    />
  );
}

/** Full rune tree as a compact icon grid with the page's picks highlighted.
 * Sub-tree columns hide the keystone row (sub picks never use it). */
function TreeColumn(props: {
  styleId: number;
  label: string;
  pickedIds: number[];
  hideKeystones?: boolean;
}) {
  const style = () => getStyle(props.styleId);
  const tree = () => getStyleTree(props.styleId);
  const rows = () => (props.hideKeystones ? tree().slice(1) : tree());
  const picked = () => new Set(props.pickedIds);

  return (
    <div class="rune-tree-col">
      <div class="rune-tree-head" style={style() ? { color: style()?.color } : undefined}>
        <Show when={style()?.icon}>
          <Icon url={style()?.icon ?? ""} class="rune-tree-head-icon" />
        </Show>
        {props.label} · {(style()?.name ?? "").toUpperCase()}
      </div>
      <Show
        when={tree().length > 0}
        fallback={
          <div class="rune-tree-row">
            <For each={props.pickedIds}>{(id) => <RuneCell perkId={id} picked={true} />}</For>
          </div>
        }
      >
        <For each={rows()}>
          {(row, i) => (
            <div class="rune-tree-row">
              <For each={row}>
                {(id) => (
                  <RuneCell
                    perkId={id}
                    picked={picked().has(id)}
                    keystone={!props.hideKeystones && i() === 0}
                  />
                )}
              </For>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

/** 3×3 stat-shard picker with the page's picks highlighted. */
function ShardGrid(props: { pickedIds: number[] }) {
  const picked = () => new Set(props.pickedIds);
  // Legacy ids (5002/5003) live outside the current rows — append them so a
  // picked legacy shard never renders as "nothing selected".
  const extras = () => {
    const known = new Set(SHARD_ROWS.flat());
    return props.pickedIds.filter((id) => !known.has(id));
  };

  return (
    <div class="rune-tree-col">
      <div class="rune-tree-head">SHARDS</div>
      <For each={SHARD_ROWS}>
        {(row) => (
          <div class="rune-tree-row">
            <For each={row}>
              {(id) => {
                const info = () => getShard(id);
                return (
                  <Icon
                    url={info().icon}
                    title={info().label}
                    class={`rune-cell rune-cell--shard ${picked().has(id) ? "is-picked" : ""}`}
                  />
                );
              }}
            </For>
          </div>
        )}
      </For>
      <Show when={extras().length > 0}>
        <div class="rune-tree-row">
          <For each={extras()}>
            {(id) => {
              const info = () => getShard(id);
              return (
                <Icon
                  url={info().icon}
                  title={info().label}
                  class="rune-cell rune-cell--shard is-picked"
                />
              );
            }}
          </For>
        </div>
      </Show>
    </div>
  );
}

function RunePage(props: { build: RuneBuild }) {
  const b = () => props.build;

  return (
    <div class="rune-compact">
      <div class="rune-compact-trees">
        <TreeColumn styleId={b().primaryStyleId} label="MAIN" pickedIds={b().primaryPerkIds} />
        <TreeColumn styleId={b().subStyleId} label="SUB" pickedIds={b().subPerkIds} hideKeystones />
        <ShardGrid pickedIds={b().shardIds} />
      </div>
      <div class="rune-compact-meta">
        <span>
          Win Rate <strong>{fmtPct(b().winRate)}</strong>
        </span>
        <span>
          Games <strong>{fmtThousands(b().games)}</strong>
        </span>
      </div>
    </div>
  );
}

function BuildSkeleton() {
  return (
    <div class="flex flex-col gap-1.5">
      <div class="hx-skel h-3.5 w-16 rounded" />
      <For each={[0, 1, 2]}>{() => <div class="hx-skel h-7 w-24 rounded-md" />}</For>
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
