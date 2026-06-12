import { HexgatePanel } from "./components/hexgate/HexgatePanel";
import { InGamePanel } from "./components/ingame/InGamePanel";
import { LpBanner } from "./components/LpBanner";
import { RuneBanner } from "./components/RuneBanner";
import { SettingsPanel } from "./components/SettingsPanel";
import { StatusChip } from "./components/StatusChip";

export function App() {
  return (
    <>
      <div class="status-chip">
        <StatusChip />
      </div>
      <LpBanner />
      <RuneBanner />
      <SettingsPanel />
      <InGamePanel />
      <HexgatePanel />
    </>
  );
}
