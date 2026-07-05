import { createEffect, Show } from "solid-js";
import { DebugPanel } from "./components/DebugPanel";
import { InGamePanel } from "./components/ingame/InGamePanel";
import { LpBanner } from "./components/LpBanner";
import { OpenLolPanel } from "./components/openlol/OpenLolPanel";
import { RuneBanner } from "./components/RuneBanner";
import { ScrollArea } from "./components/ScrollArea";
import { SettingsForm } from "./components/SettingsPanel";
import { StatusChip } from "./components/StatusChip";
import { champSelect, interactive, windowMode } from "./state/backend";
import { developerMode } from "./state/settings";

export function OverlayApp() {
  createEffect(() => {
    document.body.classList.toggle("interactive", interactive());
  });

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
    document.body.classList.toggle("interactive", interactive());
  });

  return (
    <div class="control-root">
      {pickActive() ? (
        <OpenLolPanel />
      ) : mode() === "ingame" ? (
        <main class="control-ingame">
          <ScrollArea class="h-full">
            <InGamePanel embedded />
          </ScrollArea>
        </main>
      ) : (
        <div class="control-home">
          <section class="panel control-status-panel">
            <StatusChip />
          </section>
          <section class="panel control-settings-panel">
            <ScrollArea class="h-full">
              <SettingsForm />
              <Show when={developerMode()}>
                <div class="mt-3">
                  <DebugPanel />
                </div>
              </Show>
            </ScrollArea>
          </section>
        </div>
      )}
    </div>
  );
}
