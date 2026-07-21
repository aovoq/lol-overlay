import { createEffect, createMemo, createSignal, For, Index, Show } from "solid-js";
import { assetsReady, getAbility, setIcon } from "../../assets";
import type { SkillOrder } from "../../types";

const MAX_LEVEL = 18;
const SKILL_KEYS = ["", "Q", "W", "E", "R"];

/** Ability icon with a Q/W/E/R badge; falls back to the letter alone. */
export function AbilityIcon(props: { skillId: number; championImageId: string }) {
  const [hasIcon, setHasIcon] = createSignal(false);
  const [abilityName, setAbilityName] = createSignal("");
  let imgEl!: HTMLImageElement;
  let generation = 0;

  createEffect(() => {
    assetsReady();
    const id = props.skillId;
    const champ = props.championImageId;
    const current = ++generation;
    setHasIcon(false);
    setAbilityName("");
    imgEl.style.visibility = "hidden";
    void getAbility(champ, id).then((ability) => {
      if (current !== generation || !ability) return;
      imgEl.style.visibility = "";
      setIcon(imgEl, ability.icon);
      setAbilityName(ability.name);
      setHasIcon(true);
    });
  });

  const key = () => SKILL_KEYS[props.skillId] ?? "?";
  const title = () => {
    const name = abilityName();
    return name ? `${key()} · ${name}` : key();
  };

  return (
    <span class={`skill-matrix-head ${hasIcon() ? "has-icon" : ""}`} title={title()}>
      <img ref={imgEl} class="skill-matrix-head-icon" alt="" />
      <span class="skill-matrix-head-key">{key()}</span>
    </span>
  );
}

/** Basic-skill max priority (e.g. Q > E > W), derived from levelOrder when
 * the source only provides the level-by-level order. */
function maxPriority(order: SkillOrder | null | undefined): number[] {
  const isBasic = (id: number) => id >= 1 && id <= 3;
  const maxOrder = (order?.maxOrder ?? []).filter(isBasic);
  if (maxOrder.length > 0) return maxOrder.slice(0, 3);

  const derived: number[] = [];
  for (const skillId of order?.levelOrder ?? []) {
    if (!isBasic(skillId) || derived.includes(skillId)) continue;
    derived.push(skillId);
    if (derived.length === 3) break;
  }
  return derived;
}

/** Skill-max order as icon → icon → icon (the reference's "Skill Master"). */
export function SkillMaster(props: {
  order: SkillOrder | null | undefined;
  championImageId: string;
}) {
  const ids = () => maxPriority(props.order);

  return (
    <Show when={ids().length > 0}>
      <div class="skill-master">
        <Index each={ids()}>
          {(skillId, i) => (
            <>
              <Show when={i > 0}>
                <span class="item-path-arrow" />
              </Show>
              <AbilityIcon skillId={skillId()} championImageId={props.championImageId} />
            </>
          )}
        </Index>
      </div>
    </Show>
  );
}

/** Level-by-level skill order as a Q/W/E/R × 1..18 grid — the taken cells
 * carry the champion level, mirroring stat-site build pages. */
export function SkillMatrix(props: {
  order: SkillOrder | null | undefined;
  championImageId: string;
}) {
  /** skillId → champion levels at which that skill is taken. */
  const takenAt = createMemo(() => {
    const map = new Map<number, number[]>();
    const order = props.order?.levelOrder ?? [];
    for (let i = 0; i < Math.min(order.length, MAX_LEVEL); i++) {
      const skillId = order[i];
      if (skillId < 1 || skillId > 4) continue;
      const levels = map.get(skillId) ?? [];
      levels.push(i + 1);
      map.set(skillId, levels);
    }
    return map;
  });

  const rows = createMemo(() => [1, 2, 3, 4].filter((id) => takenAt().has(id)));
  const takenSet = (skillId: number) => new Set(takenAt().get(skillId) ?? []);

  const title = () => {
    const o = props.order;
    return o && o.games > 0 ? `${Math.round(o.winRate * 100)}% WR · ${o.games} games` : "";
  };

  return (
    <Show when={rows().length > 0}>
      <div class="skill-matrix" title={title()}>
        <For each={rows()}>
          {(skillId) => (
            <div class="skill-matrix-row">
              <AbilityIcon skillId={skillId} championImageId={props.championImageId} />
              <Index each={Array.from({ length: MAX_LEVEL }, (_, i) => i + 1)}>
                {(level) => (
                  <span
                    class={`skill-matrix-cell ${takenSet(skillId).has(level()) ? "is-taken" : ""}`}
                  >
                    {takenSet(skillId).has(level()) ? level() : ""}
                  </span>
                )}
              </Index>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}
