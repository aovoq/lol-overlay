import { render } from "solid-js/web";
import { initAssets } from "./assets";
import { App } from "./App";
import "./app.css";
import {
  clampPanelToViewport,
  saveIngamePanelPosition,
} from "./lib/drag";
import { startHitRegionInterval } from "./lib/hitRegions";
import "./state/backend";
import "./state/settings";

render(() => <App />, document.getElementById("root")!);

startHitRegionInterval();
initAssets().catch(() => {});

window.addEventListener("resize", () => {
  const panel = document.querySelector<HTMLElement>(".ingame-panel");
  if (!panel) return;
  clampPanelToViewport(panel);
  saveIngamePanelPosition(panel);
});
