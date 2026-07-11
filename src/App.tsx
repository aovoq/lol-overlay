import { HashRouter, Route } from "@solidjs/router";
import { createEffect, Show } from "solid-js";
import { DesktopShell } from "./components/desktop/DesktopShell";
import {
  ChampionPage,
  ChampionsPage,
  HomePage,
  LivePage,
  NotFoundPage,
  SettingsPage,
} from "./components/desktop/Pages";
import { InGamePanel } from "./components/ingame/InGamePanel";
import { LpBanner } from "./components/LpBanner";
import { RuneBanner } from "./components/RuneBanner";
import { interactive, phase, windowMode } from "./state/backend";
import { presentationMode } from "./state/settings";

export function OverlayApp() {
  const showOverlayInGame = () =>
    presentationMode() === "overlay" && windowMode() === "overlay" && (phase()?.inGame ?? false);

  createEffect(() => {
    document.body.classList.toggle("interactive", interactive());
  });

  return (
    <>
      <LpBanner />
      <RuneBanner />
      <Show when={showOverlayInGame()}>
        <InGamePanel />
      </Show>
    </>
  );
}

export function ControlApp() {
  createEffect(() => {
    document.body.classList.toggle("interactive", interactive());
  });

  return (
    <div class="control-root">
      <HashRouter root={DesktopShell}>
        <Route path="/" component={HomePage} />
        <Route path="/champions" component={ChampionsPage} />
        <Route path="/champions/:id" component={ChampionPage} />
        <Route path="/live" component={LivePage} />
        <Route path="/settings" component={SettingsPage} />
        <Route path="*" component={NotFoundPage} />
      </HashRouter>
    </div>
  );
}
