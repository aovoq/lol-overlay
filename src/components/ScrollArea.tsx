import { ScrollArea as BaseScrollArea } from "@msviderok/base-ui-solid/scroll-area";
import type { JSX } from "solid-js";

export function ScrollArea(props: {
  class?: string;
  contentClass?: string;
  hit?: boolean;
  children: JSX.Element;
}) {
  return (
    <BaseScrollArea.Root class={`hx-scroll-root ${props.class ?? ""}`} data-hit={props.hit}>
      <BaseScrollArea.Viewport class="hx-scroll-viewport">
        <BaseScrollArea.Content class={props.contentClass}>{props.children}</BaseScrollArea.Content>
      </BaseScrollArea.Viewport>
      <BaseScrollArea.Scrollbar class="hx-scrollbar" orientation="vertical">
        <BaseScrollArea.Thumb class="hx-scrollbar-thumb" />
      </BaseScrollArea.Scrollbar>
    </BaseScrollArea.Root>
  );
}
