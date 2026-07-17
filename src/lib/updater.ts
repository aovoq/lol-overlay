import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { createSignal } from "solid-js";

const [availableUpdateVersion, setAvailableUpdateVersion] = createSignal<string | null>(null);

export { availableUpdateVersion };

/**
 * Check GitHub Releases for a newer version; on user consent download,
 * install, and relaunch. Called once from the control window at startup.
 */
export async function checkForUpdates(): Promise<void> {
  const update = await check();
  if (!update) {
    setAvailableUpdateVersion(null);
    return;
  }
  setAvailableUpdateVersion(update.version);
  const ok = window.confirm(
    `新しいバージョン ${update.version} があります。今すぐアップデートしますか？`,
  );
  if (!ok) return;
  await update.downloadAndInstall();
  await relaunch();
}
