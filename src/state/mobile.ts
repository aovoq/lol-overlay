import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import type { MobilePairingState } from "../types";

export const MOBILE_DISCONNECTED: MobilePairingState = {
  status: "disconnected",
  sessionId: "",
  viewerUrl: "",
  pairingCode: "",
  pairingCodeExpiresAt: 0,
  expiresAt: 0,
  message: "",
};

const [mobilePairing, setMobilePairing] = createSignal<MobilePairingState>(MOBILE_DISCONNECTED);
let initialized = false;

/** Start the control-window pairing snapshot/event subscription once. */
export function initMobilePairingState() {
  if (initialized) return;
  initialized = true;

  invoke<MobilePairingState>("get_mobile_pairing")
    .then(setMobilePairing)
    .catch(() => {});
  listen<MobilePairingState>("mobile-pairing", (event) => setMobilePairing(event.payload)).catch(
    () => {},
  );
}

export { mobilePairing, setMobilePairing };
