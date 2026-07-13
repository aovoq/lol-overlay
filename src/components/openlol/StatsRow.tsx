import { createMemo, For, Show } from "solid-js";
import { fmtPct, fmtThousands, getSpell } from "../../assets";
import { buildCache, buildKey } from "../../state/caches";
import {
  importSpells,
  setSpellsFlipped as persistSpellsFlipped,
  setImportSpells,
  spellsFlipped as spellsFlippedSetting,
} from "../../state/settings";
import { Icon } from "../Icon";

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

  const spells = createMemo(() => {
    const b = build();
    if (!b) return [];
    return spellsFlippedSetting() ? [...b.spellIds].reverse() : b.spellIds;
  });

  return (
    <Show when={build()}>
      {(b) => (
        <div class="flex items-center gap-2 py-2.5">
          <span class="font-bold text-sm text-hx-text">{fmtPct(b().winRate)} WR</span>
          <span class="text-hx-muted"> · {fmtThousands(b().games)} games</span>
          <span class="flex-1" />
          <div class="flex gap-1">
            <For each={spells()}>
              {(id) => (
                <Icon
                  url={getSpell(id)?.icon ?? ""}
                  class="w-[26px] h-[26px] rounded border border-hx-border"
                  title={getSpell(id)?.name ?? `Spell ${id}`}
                />
              )}
            </For>
          </div>
          <Show when={b().spellIds.length >= 2}>
            <button
              type="button"
              class="bg-none border border-hx-border rounded px-2 py-1 font-hx-display font-semibold text-[10px] tracking-widest text-hx-accent-dim hover:text-hx-accent hover:border-hx-accent-dim cursor-pointer"
              onClick={() => persistSpellsFlipped(!spellsFlippedSetting())}
            >
              FLIP
            </button>
          </Show>
          <label class="flex items-center gap-1.5 ml-1.5 text-xs text-hx-text cursor-pointer">
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
