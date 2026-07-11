import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import QRCode from "qrcode";
import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import type { MobilePairingState } from "../types";

const DISCONNECTED: MobilePairingState = {
  status: "disconnected",
  sessionId: "",
  viewerUrl: "",
  expiresAt: 0,
  message: "",
};

const RELAY_URL =
  (import.meta.env.VITE_MOBILE_RELAY_URL as string | undefined)?.trim() ||
  "https://lol-overlay-relay.voq.workers.dev";

export function MobilePairing() {
  const [state, setState] = createSignal<MobilePairingState>(DISCONNECTED);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal("");
  let canvas: HTMLCanvasElement | undefined;

  onMount(() => {
    invoke<MobilePairingState>("get_mobile_pairing")
      .then(setState)
      .catch(() => {});
    let unlisten: (() => void) | undefined;
    listen<MobilePairingState>("mobile-pairing", (event) => setState(event.payload))
      .then((dispose) => {
        unlisten = dispose;
      })
      .catch(() => {});
    onCleanup(() => unlisten?.());
  });

  createEffect(() => {
    const viewerUrl = state().viewerUrl;
    if (!canvas || !viewerUrl) return;
    QRCode.toCanvas(canvas, viewerUrl, {
      width: 132,
      margin: 1,
      color: { dark: "#090b10", light: "#ffffff" },
      errorCorrectionLevel: "M",
    }).catch(() => setError("QRコードを生成できませんでした"));
  });

  const start = async () => {
    setLoading(true);
    setError("");
    try {
      setState(await invoke<MobilePairingState>("start_mobile_pairing", { relayUrl: RELAY_URL }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  const stop = async () => {
    setState(await invoke<MobilePairingState>("stop_mobile_pairing"));
    setError("");
  };

  const connected = () => state().status !== "disconnected";
  const expires = () =>
    state().expiresAt
      ? new Date(state().expiresAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
      : "";

  return (
    <div class="flex flex-col gap-3">
      <Show
        when={connected()}
        fallback={
          <button
            type="button"
            class="hx-primary-button min-h-9 rounded px-3 text-[11px] font-bold cursor-pointer disabled:opacity-50"
            disabled={loading()}
            onClick={start}
          >
            {loading() ? "CONNECTING" : "CONNECT IPHONE"}
          </button>
        }
      >
        <div class="mobile-pairing-grid">
          <div class="mobile-qr-shell">
            <canvas ref={canvas} width="132" height="132" aria-label="iPhone connection QR code" />
          </div>
          <div class="flex min-w-0 flex-1 flex-col justify-between gap-2">
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2 text-[11px] font-bold text-hx-text">
                <span
                  class={`h-2 w-2 rounded-full ${state().status === "error" ? "bg-hx-red" : "bg-hx-up"}`}
                />
                {state().status === "error" ? "RELAY ERROR" : "PAIRING READY"}
              </div>
              <span class="text-[10px] text-hx-muted">有効期限 {expires()}</span>
            </div>
            <button
              type="button"
              class="min-h-8 rounded border border-hx-border bg-transparent px-2 text-[10px] font-bold text-hx-muted cursor-pointer hover:text-hx-text"
              onClick={stop}
            >
              DISCONNECT
            </button>
          </div>
        </div>
      </Show>
      <Show when={state().message && state().status === "error"}>
        <span class="text-[10px] text-hx-red">{state().message}</span>
      </Show>
      <Show when={error()}>
        <span class="break-all text-[10px] text-hx-red">{error()}</span>
      </Show>
    </div>
  );
}
