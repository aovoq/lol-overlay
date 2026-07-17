import { For, Show } from "solid-js";
import type { ThreatProfile } from "../../types";

function threatColor(kind: string): string {
  switch (kind) {
    case "ad":
      return "text-hx-physical";
    case "ap":
      return "text-hx-magic";
    case "tank":
      return "text-hx-durable";
    default:
      return "";
  }
}

/** Enemy team composition chips (AD/AP/TANK counts + CC warning). */
export function ThreatChips(props: { threats?: ThreatProfile | null }) {
  const chips = () => {
    const t = props.threats;
    if (!t) return [];
    const list: { kind: string; count?: number; label: string; cc?: boolean }[] = [
      { kind: "ad", count: t.adCount, label: "AD" },
      { kind: "ap", count: t.apCount, label: "AP" },
      { kind: "tank", count: t.tankCount, label: "TANK" },
    ];
    if (t.ccHeavy) list.push({ kind: "cc", label: "CC HEAVY", cc: true });
    return list;
  };

  return (
    <div class="flex flex-wrap gap-1">
      <For each={chips()}>
        {(chip) => (
          <span
            class={`px-[7px] py-0.5 border rounded-[3px] bg-hx-bg-raised text-[10px] font-semibold tracking-[0.06em] ${
              chip.cc ? "text-hx-red border-hx-red-soft" : "text-hx-muted border-hx-border"
            }`}
          >
            <Show when={chip.count !== undefined} fallback={chip.label}>
              <b class={`font-bold ${threatColor(chip.kind)}`}>{chip.count}</b>
              {` ${chip.label}`}
            </Show>
          </span>
        )}
      </For>
    </div>
  );
}
