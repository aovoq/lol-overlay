import { createEffect } from "solid-js";
import { setIcon } from "../assets";

export function Icon(props: {
  url: string;
  class?: string;
  title?: string;
  alt?: string;
  style?: Record<string, string>;
}) {
  let el!: HTMLImageElement;
  createEffect(() => setIcon(el, props.url));
  return (
    <img
      ref={el}
      class={props.class}
      title={props.title}
      alt={props.alt ?? ""}
      style={props.style}
    />
  );
}
