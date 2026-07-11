import {
  isRelayMessage,
  type PairingLink,
  RELAY_SUBPROTOCOL,
  type RelayMessage,
  viewerCommandUrl,
  viewerWebSocketUrl,
} from "@lol-overlay/protocol";
import { useCallback, useEffect, useRef, useState } from "react";

export type ConnectionState = "idle" | "connecting" | "waiting" | "live" | "reconnecting" | "error";

const DD = "https://ddragon.leagueoflegends.com";
const MAX_OPEN_RETRIES = 3;
const MAX_LIVE_RETRIES = 8;

export function useDataDragonVersion(): string {
  const [version, setVersion] = useState("");
  useEffect(() => {
    fetch(`${DD}/api/versions.json`)
      .then((response) => response.json() as Promise<string[]>)
      .then((versions) => setVersion(versions[0] ?? ""))
      .catch(() => setVersion(""));
  }, []);
  return version;
}

export function useRelay(link: PairingLink) {
  const [state, setState] = useState<ConnectionState>("connecting");
  const [snapshot, setSnapshot] = useState<RelayMessage & { type: "snapshot" }>();
  const [receivedAt, setReceivedAt] = useState(0);
  const [error, setError] = useState("");
  const consecutiveFailures = useRef(0);
  const everHealthy = useRef(false);

  useEffect(() => {
    let active = true;
    let socket: WebSocket | undefined;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;
    consecutiveFailures.current = 0;
    everHealthy.current = false;

    const markHealthy = () => {
      consecutiveFailures.current = 0;
      everHealthy.current = true;
    };

    const connect = () => {
      if (!active) return;
      setState(consecutiveFailures.current ? "reconnecting" : "connecting");
      try {
        socket = new WebSocket(viewerWebSocketUrl(link), [
          RELAY_SUBPROTOCOL,
          `auth.${link.viewerToken}`,
        ]);
      } catch {
        setError("接続情報が不正です");
        setState("error");
        return;
      }
      socket.onopen = () => {
        setError("");
        setState("waiting");
        // Do not reset consecutiveFailures here — open-then-immediate-close
        // loops would otherwise reconnect forever.
      };
      socket.onmessage = (event) => {
        try {
          const message: unknown = JSON.parse(String(event.data));
          if (!isRelayMessage(message)) throw new Error("invalid relay message");
          if (message.type === "snapshot") {
            markHealthy();
            setSnapshot(message);
            setReceivedAt(Date.now());
            setState(message.snapshot.game ? "live" : "waiting");
          } else if (message.type === "status") {
            markHealthy();
            setState("waiting");
          } else if (message.type === "error") {
            setError(message.message);
            setState("error");
          }
        } catch {
          setError("Relayから不正なデータを受信しました");
          setState("error");
        }
      };
      socket.onerror = () => setError("Relayへ接続できません");
      socket.onclose = (event) => {
        if (!active) return;
        // 4001 expired, 4002 revoked by producer, 4003 reserved for unauthorized.
        if (event.code === 4001 || event.code === 4002 || event.code === 4003) {
          setError(
            event.code === 4002
              ? "接続セッションが切断されました"
              : "接続セッションの有効期限が切れました",
          );
          setState("error");
          return;
        }

        consecutiveFailures.current += 1;
        const limit = everHealthy.current ? MAX_LIVE_RETRIES : MAX_OPEN_RETRIES;
        if (consecutiveFailures.current > limit) {
          setError("Relayへ接続できません");
          setState("error");
          return;
        }

        setState("reconnecting");
        const delay = Math.min(15_000, 500 * 2 ** consecutiveFailures.current);
        retryTimer = setTimeout(connect, delay);
      };
    };

    connect();
    return () => {
      active = false;
      if (retryTimer) clearTimeout(retryTimer);
      socket?.close(1000, "screen closed");
    };
  }, [link]);

  const respondToReadyCheck = useCallback(
    async (response: "accept" | "decline") => {
      const result = await fetch(viewerCommandUrl(link), {
        method: "POST",
        headers: {
          authorization: `Bearer ${link.viewerToken}`,
          "content-type": "application/json",
        },
        body: JSON.stringify({
          type: "readyCheckResponse",
          requestId: `${Date.now()}-${Math.random().toString(36).slice(2)}`,
          response,
        }),
      });
      if (!result.ok) throw new Error(`command failed: ${result.status}`);
    },
    [link],
  );

  return {
    state,
    snapshot: snapshot?.snapshot ?? null,
    receivedAt,
    error,
    respondToReadyCheck,
  };
}
