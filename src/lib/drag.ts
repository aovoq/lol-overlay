import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setIngamePos } from "../state/layout";
import type { WindowMode } from "../types";
import { reportHitRegions } from "./hitRegions";

const appWindow = getCurrentWindow();
let controlWindowGeometrySaveTimer: number | undefined;

export function shouldStartDrag(event: PointerEvent): boolean {
  if (event.button !== 0) return false;
  const target = event.target;
  if (!(target instanceof Element)) return true;
  return !target.closest("button, input, label, select, textarea, a");
}

export function applyPanelPosition(panel: HTMLElement, left: number, top: number) {
  panel.style.left = `${Math.max(0, left)}px`;
  panel.style.top = `${Math.max(0, top)}px`;
  panel.style.right = "auto";
  panel.style.bottom = "auto";
}

export function clampPanelToViewport(panel: HTMLElement) {
  const rect = panel.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return;

  const left = Math.min(Math.max(0, rect.left), Math.max(0, window.innerWidth - rect.width));
  const top = Math.min(Math.max(0, rect.top), Math.max(0, window.innerHeight - rect.height));
  applyPanelPosition(panel, left, top);
}

export function saveIngamePanelPosition(panel: HTMLElement) {
  const rect = panel.getBoundingClientRect();
  const left = Math.round(rect.left);
  const top = Math.round(rect.top);
  setIngamePos({ left, top });
  invoke("set_ingame_panel_position", { left, top }).catch(() => {});
}

async function saveControlWindowGeometry(mode: WindowMode) {
  const [scale, position, size] = await Promise.all([
    appWindow.scaleFactor(),
    appWindow.outerPosition(),
    appWindow.innerSize(),
  ]);
  await invoke("set_control_window_geometry", {
    mode,
    x: Math.round(position.x / scale),
    y: Math.round(position.y / scale),
    width: Math.round(size.width / scale),
    height: Math.round(size.height / scale),
  });
}

export function initWindowDrag(header: HTMLElement | undefined) {
  header?.addEventListener("pointerdown", (event) => {
    if (!shouldStartDrag(event)) return;
    event.preventDefault();
    appWindow.startDragging().catch(() => {});
  });
}

export function initControlWindowGeometrySave(getMode: () => WindowMode) {
  const scheduleSave = () => {
    if (controlWindowGeometrySaveTimer) window.clearTimeout(controlWindowGeometrySaveTimer);
    controlWindowGeometrySaveTimer = window.setTimeout(() => {
      saveControlWindowGeometry(getMode()).catch(() => {});
    }, 250);
  };

  appWindow
    .onMoved(() => {
      scheduleSave();
    })
    .catch(() => {});
  appWindow
    .onResized(() => {
      scheduleSave();
    })
    .catch(() => {});
}

export function initPanelDrag(panel: HTMLElement, handle: HTMLElement | undefined) {
  handle?.addEventListener("pointerdown", (event) => {
    if (!shouldStartDrag(event)) return;
    event.preventDefault();

    const rect = panel.getBoundingClientRect();
    const startX = event.clientX;
    const startY = event.clientY;
    const startLeft = rect.left;
    const startTop = rect.top;

    panel.style.left = `${startLeft}px`;
    panel.style.top = `${startTop}px`;
    panel.style.right = "auto";
    panel.style.bottom = "auto";
    handle.setPointerCapture(event.pointerId);
    invoke("set_drag_active", { active: true }).catch(() => {});

    const onPointerMove = (moveEvent: PointerEvent) => {
      const maxLeft = Math.max(0, window.innerWidth - rect.width);
      const maxTop = Math.max(0, window.innerHeight - rect.height);
      const left = Math.min(Math.max(0, startLeft + moveEvent.clientX - startX), maxLeft);
      const top = Math.min(Math.max(0, startTop + moveEvent.clientY - startY), maxTop);
      panel.style.left = `${left}px`;
      panel.style.top = `${top}px`;
    };

    const stopDragging = () => {
      if (handle.hasPointerCapture(event.pointerId)) {
        handle.releasePointerCapture(event.pointerId);
      }
      handle.removeEventListener("pointermove", onPointerMove);
      handle.removeEventListener("pointerup", stopDragging);
      handle.removeEventListener("pointercancel", stopDragging);
      clampPanelToViewport(panel);
      saveIngamePanelPosition(panel);
      invoke("set_drag_active", { active: false }).catch(() => {});
      reportHitRegions();
    };

    handle.addEventListener("pointermove", onPointerMove);
    handle.addEventListener("pointerup", stopDragging);
    handle.addEventListener("pointercancel", stopDragging);
  });
}
