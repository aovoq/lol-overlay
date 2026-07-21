import { createMemo, For, Show } from "solid-js";
import { fmtPct, fmtThousands, getSpell } from "../../assets";
import { buildCache, buildKey, tierCache } from "../../state/caches";
import {
  importSpells,
  setSpellsFlipped as persistSpellsFlipped,
  setImportSpells,
  spellsFlipped as spellsFlippedSetting,
} from "../../state/settings";
import { Icon } from "../Icon";

function StatCell(props: { label: string; value: string }) {
  return (
    <div class="stat-cell">
      <span>{props.label}</span>
      <strong>{props.value}</strong>
    </div>
  );
}

export function StatsRow(props: { championId: number; role: string; enemyId?: number | null }) {
  const role = createMemo(() => props.role);
  const target = createMemo(() =>
    props.championId > 0 ? { champ: props.championId, enemy: props.enemyId ?? null } : null,
  );
  const cacheKey = createMemo(() => {
    const t = target();
    return t ? buildKey(t.champ, role(), t.enemy) : "";
  });
  const entry = createMemo(() => (cacheKey() ? buildCache.get(cacheKey()) : null));
  const build = createMemo(() => {
    const e = entry();
    if (e?.state !== "ok") return null;
    return e.value;
  });

  /** Role-wide champion stats (WR/pick/ban) — the rune page's own WR/games
   * are shown under the rune grid instead. */
  const tierEntry = createMemo(() => {
    if (props.championId <= 0 || !role()) return null;
    const e = tierCache.get(role());
    if (e.state !== "ok") return null;
    return e.value.find((t) => t.championId === props.championId) ?? null;
  });

  const winRate = createMemo(() => tierEntry()?.winRate ?? build()?.winRate ?? null);
  const games = createMemo(() => tierEntry()?.games ?? build()?.games ?? null);

  const spells = createMemo(() => {
    const b = build();
    if (!b) return [];
    return spellsFlippedSetting() ? [...b.spellIds].reverse() : b.spellIds;
  });

  return (
    <Show when={build()}>
      {(b) => (
        <div class="stat-strip">
          <Show when={winRate() !== null}>
            <StatCell label="WIN RATE" value={fmtPct(winRate() ?? 0)} />
          </Show>
          <Show when={tierEntry()}>
            {(t) => (
              <>
                <StatCell label="PICK RATE" value={fmtPct(t().pickRate)} />
                <StatCell label="BAN RATE" value={fmtPct(t().banRate)} />
              </>
            )}
          </Show>
          <Show when={games() !== null}>
            <StatCell label="GAMES" value={fmtThousands(games() ?? 0)} />
          </Show>

          <span class="flex-1" />

          <div class="flex gap-1">
            <For each={spells()}>
              {(id) => (
                <Icon
                  url={getSpell(id)?.icon ?? ""}
                  class="w-[22px] h-[22px] rounded border border-hx-border"
                  title={getSpell(id)?.name ?? `Spell ${id}`}
                />
              )}
            </For>
          </div>
          <Show when={b().spellIds.length >= 2}>
            <button
              type="button"
              class="bg-none border border-hx-border rounded px-1.5 py-0.5 font-hx-display font-semibold text-[9px] tracking-widest text-hx-accent-dim hover:text-hx-accent hover:border-hx-accent-dim cursor-pointer"
              onClick={() => persistSpellsFlipped(!spellsFlippedSetting())}
            >
              FLIP
            </button>
          </Show>
          <label class="flex items-center gap-1 ml-1 text-[11px] text-hx-text cursor-pointer">
            <input
              type="checkbox"
              checked={importSpells()}
              onChange={(e) => setImportSpells(e.currentTarget.checked)}
              class="accent-hx-accent"
            />
            <span>Spells</span>
          </label>
        </div>
      )}
    </Show>
  );
}
