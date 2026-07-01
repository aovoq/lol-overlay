import { createMemo, For, Show } from "solid-js";
import { fmtPct, fmtThousands, getSpell } from "../../assets";
import { activeTab, champSelect, hoverChampId, selectedRole, vsEnemyId } from "../../state/backend";
import { buildCache, buildKey } from "../../state/caches";
import {
  importSpells,
  setSpellsFlipped as persistSpellsFlipped,
  setImportSpells,
  spellsFlipped as spellsFlippedSetting,
} from "../../state/settings";
import { Icon } from "../Icon";

function effectiveRole() {
  const cs = champSelect();
  return cs?.myRole || selectedRole();
}

function displayedTarget(): { champ: number; enemy: number | null } | null {
  if (hoverChampId()) return { champ: hoverChampId(), enemy: null };
  const my = champSelect()?.myChampionId ?? 0;
  if (!my) return null;
  return {
    champ: my,
    enemy: activeTab() === "vs" && vsEnemyId() ? vsEnemyId() : null,
  };
}

export function StatsRow() {
  const role = createMemo(() => effectiveRole());
  const target = createMemo(() => displayedTarget());
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
              class="bg-none border border-hx-border rounded px-2 py-1 font-hx-serif font-semibold text-[10px] tracking-widest text-hx-gold-dim hover:text-hx-gold hover:border-hx-gold-dim cursor-pointer"
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
              class="accent-hx-gold"
            />
            <span>Spells</span>
          </label>
        </div>
      )}
    </Show>
  );
}
