import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import QRCode from "qrcode";
import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import type { MobilePairingState } from "../types";

const DISCONNECTED: MobilePairingState = {
  status: "disconnected",
  sessionId: "",
  viewerUrl: "",
  pairingCode: "",
  pairingCodeExpiresAt: 0,
  expiresAt: 0,
  message: "",
};

const RELAY_URL = (import.meta.env.VITE_MOBILE_RELAY_URL ?? "").trim();

export function MobilePairing() {
  const [state, setState] = createSignal<MobilePairingState>(DISCONNECTED);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal("");
  const [now, setNow] = createSignal(Date.now());
  const [canvas, setCanvas] = createSignal<HTMLCanvasElement | undefined>();

  onMount(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    invoke<MobilePairingState>("get_mobile_pairing")
      .then((value) => {
        if (!cancelled) setState(value);
      })
      .catch(() => {});

    listen<MobilePairingState>("mobile-pairing", (event) => setState(event.payload))
      .then((dispose) => {
        if (cancelled) {
          dispose();
          return;
        }
        unlisten = dispose;
      })
      .catch(() => {});

    const timer = window.setInterval(() => setNow(Date.now()), 15_000);

    onCleanup(() => {
      cancelled = true;
      unlisten?.();
      window.clearInterval(timer);
    });
  });

  createEffect(() => {
    const el = canvas();
    const viewerUrl = state().viewerUrl;
    if (!el || !viewerUrl) return;
    QRCode.toCanvas(el, viewerUrl, {
      width: 132,
      margin: 1,
      color: { dark: "#090b10", light: "#ffffff" },
      errorCorrectionLevel: "M",
    }).catch(() => setError("QRコードを生成できませんでした"));
  });

  const start = async () => {
    if (!RELAY_URL) {
      setError("VITE_MOBILE_RELAY_URL が未設定です（.env.example を参照）");
      return;
    }
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
    setLoading(true);
    try {
      setState(await invoke<MobilePairingState>("stop_mobile_pairing"));
      setError("");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  const connected = () => state().status !== "disconnected";
  const expired = () => {
    const expiresAt = state().expiresAt;
    return expiresAt > 0 && expiresAt <= now();
  };
  const expires = () =>
    state().expiresAt
      ? new Date(state().expiresAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
      : "";

  createEffect(() => {
    if (!connected() || !expired() || loading()) return;
    void stop();
  });

  return (
    <div class="flex flex-col gap-3">
      <Show
        when={connected()}
        fallback={
          <button
            type="button"
            class="hx-primary-button min-h-9 rounded px-3 text-[11px] font-bold cursor-pointer disabled:opacity-50"
            disabled={loading() || !RELAY_URL}
            onClick={start}
          >
            {loading() ? "CONNECTING" : "CONNECT IPHONE"}
          </button>
        }
      >
        <div class="mobile-pairing-grid">
          <div class="mobile-qr-shell">
            <canvas
              ref={setCanvas}
              width="132"
              height="132"
              aria-label="iPhone connection QR code"
            />
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
              <Show when={state().pairingCode}>
                <span class="font-mono text-2xl font-bold tracking-[0.18em] text-hx-text">
                  {state().pairingCode}
                </span>
                <span class="text-[10px] text-hx-muted">iPhoneに6桁のコードを入力</span>
                <span class="text-[10px] text-hx-muted">
                  コードは
                  {new Date(state().pairingCodeExpiresAt).toLocaleTimeString([], {
                    hour: "2-digit",
                    minute: "2-digit",
                  })}
                  まで有効
                </span>
              </Show>
              <Show when={state().message}>
                <span class="text-[10px] text-hx-muted">{state().message}</span>
              </Show>
            </div>
            <button
              type="button"
              class="min-h-8 rounded border border-hx-border bg-transparent px-2 text-[10px] font-bold text-hx-muted cursor-pointer hover:text-hx-text disabled:opacity-50"
              disabled={loading()}
              onClick={stop}
            >
              DISCONNECT
            </button>
          </div>
        </div>
      </Show>
      <Show when={error()}>
        <span class="break-all text-[10px] text-hx-red">{error()}</span>
      </Show>
      <Show when={!RELAY_URL}>
        <span class="text-[10px] text-hx-muted">
          `.env` に `VITE_MOBILE_RELAY_URL` を設定してください
        </span>
      </Show>
    </div>
  );
}
