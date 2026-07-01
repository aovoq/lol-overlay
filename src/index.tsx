import { render } from "solid-js/web";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { initAssets } from "./assets";
import { ControlApp, OverlayApp } from "./App";
import "./app.css";
import {
  clampPanelToViewport,
  saveIngamePanelPosition,
} from "./lib/drag";
import { startHitRegionInterval } from "./lib/hitRegions";
import "./state/backend";
import "./state/settings";

const windowLabel = getCurrentWindow().label;
const isControl = windowLabel === "control";

document.body.classList.toggle("control-window", isControl);
document.body.classList.toggle("overlay-window", !isControl);

render(
  () => (isControl ? <ControlApp /> : <OverlayApp />),
  document.getElementById("root")!,
);

if (!isControl) startHitRegionInterval();
initAssets().catch(() => {});

window.addEventListener("resize", () => {
  if (isControl) return;
  const panel = document.querySelector<HTMLElement>(".ingame-panel");
  if (!panel) return;
  clampPanelToViewport(panel);
  saveIngamePanelPosition(panel);
});
