import { createEffect, Show } from "solid-js";
import { OpenLolPanel } from "./components/openlol/OpenLolPanel";
import { InGamePanel } from "./components/ingame/InGamePanel";
import { LpBanner } from "./components/LpBanner";
import { RuneBanner } from "./components/RuneBanner";
import { SettingsForm } from "./components/SettingsPanel";
import { StatusChip } from "./components/StatusChip";
import { champSelect, windowMode } from "./state/backend";

export function OverlayApp() {
  return (
    <>
      <LpBanner />
      <RuneBanner />
      <Show when={windowMode() !== "ingame"}>
        <InGamePanel />
      </Show>
    </>
  );
}

export function ControlApp() {
  const mode = () => windowMode();
  const pickActive = () => mode() === "champselect" && (champSelect()?.active ?? true);
  createEffect(() => {
    document.body.classList.toggle("champselect", pickActive());
    document.body.classList.toggle("ingame-window", mode() === "ingame");
  });

  return (
    <div class="control-root">
      {pickActive() ? (
        <OpenLolPanel />
      ) : mode() === "ingame" ? (
        <main class="control-ingame">
          <InGamePanel embedded />
        </main>
      ) : (
        <div class="control-home">
          <section class="panel control-status-panel">
            <StatusChip />
          </section>
          <section class="panel control-settings-panel">
            <SettingsForm />
          </section>
        </div>
      )}
    </div>
  );
}
