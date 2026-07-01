import { createEffect } from "solid-js";
import { HexgatePanel } from "./components/hexgate/HexgatePanel";
import { InGamePanel } from "./components/ingame/InGamePanel";
import { LpBanner } from "./components/LpBanner";
import { RuneBanner } from "./components/RuneBanner";
import { SettingsForm } from "./components/SettingsPanel";
import { StatusChip } from "./components/StatusChip";
import { champSelect } from "./state/backend";

export function OverlayApp() {
  return (
    <>
      <LpBanner />
      <RuneBanner />
      <InGamePanel />
    </>
  );
}

export function ControlApp() {
  const pickActive = () => champSelect()?.active ?? false;
  createEffect(() => {
    document.body.classList.toggle("champselect", pickActive());
  });

  return (
    <div class="control-root">
      {pickActive() ? (
        <HexgatePanel />
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
