import { createEffect, createSignal, Index, Show } from "solid-js";
import { assetsReady, getAbility, setIcon } from "../../assets";
import type { SkillOrder as SkillOrderType } from "../../types";

function skillLabel(skillId: number): string {
  return ["", "Q", "W", "E", "R"][skillId] ?? "";
}

function isBasicSkill(skillId: number): boolean {
  return skillId >= 1 && skillId <= 3;
}

function skillOrderIds(order: SkillOrderType | null | undefined): number[] {
  const maxOrder = order?.maxOrder.filter(isBasicSkill) ?? [];
  if (maxOrder.length > 0) return maxOrder.slice(0, 3);

  const derived: number[] = [];
  for (const skillId of order?.levelOrder ?? []) {
    if (!isBasicSkill(skillId) || derived.includes(skillId)) continue;
    derived.push(skillId);
    if (derived.length === 3) break;
  }
  return derived;
}

function SkillCard(props: { skillId: number; championImageId: string }) {
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
      if (current !== generation) return;
      if (!ability) return;
      imgEl.style.visibility = "";
      setIcon(imgEl, ability.icon);
      setAbilityName(ability.name);
      setHasIcon(true);
    });
  });

  const label = () => skillLabel(props.skillId);
  const title = () => {
    const name = abilityName();
    return name ? `${label()} · ${name}` : label();
  };

  return (
    <span
      class={`skill-card relative w-[38px] h-[38px] flex items-center justify-center overflow-hidden border rounded-[5px] bg-hx-bg-raised ${
        hasIcon() ? "border-hx-gold-dim" : "border-hx-border"
      }`}
      title={title()}
    >
      <img
        ref={imgEl}
        class="skill-card-icon w-full h-full object-cover saturate-[0.96] contrast-[1.08] brightness-90"
        alt=""
      />
      <span
        class={`skill-card-key absolute flex items-center justify-center text-hx-gold font-hx-display font-bold leading-none ${
          hasIcon()
            ? "skill-card-key--icon inset-auto right-0 bottom-0 w-[17px] h-[15px] border-t border-l rounded-tl text-[10px] text-hx-text"
            : "inset-0 text-[17px]"
        }`}
      >
        {label()}
      </span>
    </span>
  );
}

export function SkillOrder(props: {
  order: SkillOrderType | null | undefined;
  championImageId: string;
}) {
  const ids = () => skillOrderIds(props.order);
  const title = () => {
    const o = props.order;
    return o && o.games > 0 ? `${Math.round(o.winRate * 100)}% WR · ${o.games} games` : "";
  };

  return (
    <Show when={ids().length > 0}>
      <div
        class="skill-order-row flex items-center gap-3 px-3 py-2 pb-2.5 border-b border-hx-border"
        title={title()}
      >
        <div class="skill-label flex-none w-28 h-[38px] flex items-center box-border px-2.5 border border-hx-border rounded text-hx-gold-dim font-hx-display text-[10px] font-semibold leading-tight tracking-[0.14em] uppercase whitespace-nowrap">
          Skill Order
        </div>
        <div class="flex items-center gap-2">
          <Index each={ids()}>
            {(skillId, i) => (
              <>
                <SkillCard skillId={skillId()} championImageId={props.championImageId} />
                <Show when={i < ids().length - 1}>
                  <span class="skill-arrow" />
                </Show>
              </>
            )}
          </Index>
        </div>
      </div>
    </Show>
  );
}
