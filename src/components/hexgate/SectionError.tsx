import { Show } from "solid-js";

export function SectionError(props: { message: string; onRetry: () => void }) {
  return (
    <div class="flex flex-wrap items-center gap-2 px-2 py-2.5 text-xs text-hx-muted">
      Couldn't load data{" "}
      <button
        class="bg-none border-none p-0 text-xs text-hx-gold underline cursor-pointer"
        onClick={() => props.onRetry()}
      >
        Retry
      </button>
      <Show when={props.message}>
        <div class="basis-full wrap-anywhere text-[rgba(230,217,181,0.62)] font-mono text-[10px] leading-snug">
          {props.message}
        </div>
      </Show>
    </div>
  );
}
