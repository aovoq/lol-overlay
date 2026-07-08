import { getCurrentWindow } from "@tauri-apps/api/window";
import { render } from "solid-js/web";
import { ControlApp, OverlayApp } from "./App";
import { initAssets } from "./assets";
import "./app.css";
import {
  clampPanelToViewport,
  initControlWindowGeometrySave,
  saveIngamePanelPosition,
} from "./lib/drag";
import { startHitRegionInterval } from "./lib/hitRegions";
import { checkForUpdates } from "./lib/updater";
import { windowMode } from "./state/backend";
import "./state/settings";

const windowLabel = getCurrentWindow().label;
const isControl = windowLabel === "control";

document.body.classList.toggle("control-window", isControl);
document.body.classList.toggle("overlay-window", !isControl);

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

render(() => (isControl ? <ControlApp /> : <OverlayApp />), root);

if (isControl) {
  initControlWindowGeometrySave(() => windowMode());
  checkForUpdates().catch(() => {});
} else {
  startHitRegionInterval();
}
initAssets().catch(() => {});

window.addEventListener("resize", () => {
  if (isControl) return;
  const panel = document.querySelector<HTMLElement>(".ingame-panel");
  if (!panel) return;
  clampPanelToViewport(panel);
  saveIngamePanelPosition(panel);
});
