import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";
import type { PanelPosition, UiLayout } from "../types";

const [ingameCollapsed, setIngameCollapsedState] = createSignal(false);
const [ingamePos, setIngamePos] = createSignal<PanelPosition | null>(null);

export { ingameCollapsed, setIngameCollapsedState, ingamePos, setIngamePos };

export function setIngameCollapsed(collapsed: boolean) {
  setIngameCollapsedState(collapsed);
  invoke("set_ingame_collapsed", { collapsed }).catch(() => {});
}

invoke<UiLayout>("get_ui_layout")
  .then((layout) => {
    if (layout.ingamePanel) setIngamePos(layout.ingamePanel);
    if (layout.ingameCollapsed !== undefined) {
      setIngameCollapsedState(layout.ingameCollapsed);
    }
  })
  .catch(() => {});
