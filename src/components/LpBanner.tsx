import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { fmtTier } from "../lib/openlol";
import { lpChange } from "../state/backend";

export function LpBanner() {
  const [visible, setVisible] = createSignal(false);
  const [result, setResult] = createSignal("");
  const [detail, setDetail] = createSignal("");
  const [variant, setVariant] = createSignal<"win" | "loss">("win");
  let timer: number | undefined;

  createEffect(() => {
    const e = lpChange();
    if (!e) return;

    const division = e.division && e.division !== "NA" ? ` ${e.division}` : "";
    const rankNow = `${fmtTier(e.tier)}${division} · ${e.lp} LP`;

    if (e.rankChange === "promoted") {
      setVariant("win");
      setResult(`昇格! ${fmtTier(e.tier)}${division}`);
      setDetail(`${e.lp} LP スタート`);
    } else if (e.rankChange === "demoted") {
      setVariant("loss");
      setResult(`降格 ${fmtTier(e.tier)}${division}`);
      setDetail(`${e.lp} LP`);
    } else {
      setVariant(e.win ? "win" : "loss");
      const sign = e.lpDelta >= 0 ? "+" : "";
      setResult(`${e.win ? "VICTORY" : "DEFEAT"} ${sign}${e.lpDelta} LP`);
      setDetail(rankNow);
    }

    setVisible(true);
    if (timer) window.clearTimeout(timer);
    timer = window.setTimeout(() => setVisible(false), 12000);
  });

  onCleanup(() => {
    if (timer) window.clearTimeout(timer);
  });

  return (
    <Show when={visible()}>
      <div
        class={`panel fixed top-[18px] left-1/2 -translate-x-1/2 flex gap-2.5 items-center border-hx-gold-dim border-l-[3px] ${
          variant() === "win" ? "border-l-hx-up" : "border-l-hx-red"
        }`}
      >
        <div class="flex flex-col leading-snug">
          <strong
            class={`font-hx-serif text-[15px] tracking-wide ${
              variant() === "win" ? "text-hx-up" : "text-hx-red"
            }`}
          >
            {result()}
          </strong>
          <span class="text-hx-muted">{detail()}</span>
        </div>
      </div>
    </Show>
  );
}
