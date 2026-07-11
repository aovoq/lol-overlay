import {
  isMobileSnapshot,
  type PairingSession,
  RELAY_SUBPROTOCOL,
  type RelayMessage,
} from "@lol-overlay/protocol";

interface Env {
  SESSIONS: DurableObjectNamespace;
  MOBILE_APP_URL?: string;
}

interface SessionMetadata {
  producerTokenHash: string;
  viewerTokenHash: string;
  expiresAt: number;
}

const SESSION_LIFETIME_MS = 4 * 60 * 60 * 1000;
const MAX_SNAPSHOT_BYTES = 64 * 1024;

function json(data: unknown, init: ResponseInit = {}): Response {
  const headers = new Headers(init.headers);
  headers.set("content-type", "application/json; charset=utf-8");
  headers.set("cache-control", "no-store");
  return new Response(JSON.stringify(data), { ...init, headers });
}

function randomToken(bytes = 32): string {
  const value = new Uint8Array(bytes);
  crypto.getRandomValues(value);
  return btoa(String.fromCharCode(...value))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

async function hashToken(token: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(token));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function bearerToken(request: Request): string | null {
  const value = request.headers.get("authorization");
  return value?.startsWith("Bearer ") ? value.slice(7) : null;
}

function sessionStub(env: Env, sessionId: string): DurableObjectStub {
  return env.SESSIONS.get(env.SESSIONS.idFromName(sessionId));
}

async function createSession(request: Request, env: Env): Promise<Response> {
  const sessionId = randomToken(18);
  const producerToken = randomToken();
  const viewerToken = randomToken();
  const expiresAt = Date.now() + SESSION_LIFETIME_MS;

  const metadata: SessionMetadata = {
    producerTokenHash: await hashToken(producerToken),
    viewerTokenHash: await hashToken(viewerToken),
    expiresAt,
  };
  const initialized = await sessionStub(env, sessionId).fetch("https://session.internal/init", {
    method: "POST",
    body: JSON.stringify(metadata),
  });
  if (!initialized.ok) return json({ error: "session_init_failed" }, { status: 502 });

  const relayUrl = new URL(request.url).origin;
  const viewerUrl = new URL(env.MOBILE_APP_URL ?? "loloverlay://pair");
  viewerUrl.searchParams.set("relay", relayUrl);
  viewerUrl.searchParams.set("session", sessionId);
  viewerUrl.hash = new URLSearchParams({ token: viewerToken }).toString();

  const response: PairingSession = {
    sessionId,
    producerToken,
    viewerUrl: viewerUrl.href,
    expiresAt,
  };
  return json(response, { status: 201 });
}

async function publishSnapshot(request: Request, env: Env, sessionId: string): Promise<Response> {
  const token = bearerToken(request);
  if (!token) return json({ error: "unauthorized" }, { status: 401 });

  const body = await request.text();
  if (new TextEncoder().encode(body).byteLength > MAX_SNAPSHOT_BYTES) {
    return json({ error: "snapshot_too_large" }, { status: 413 });
  }

  let snapshot: unknown;
  try {
    snapshot = JSON.parse(body);
  } catch {
    return json({ error: "invalid_json" }, { status: 400 });
  }
  if (!isMobileSnapshot(snapshot)) return json({ error: "invalid_snapshot" }, { status: 400 });

  return sessionStub(env, sessionId).fetch("https://session.internal/snapshot", {
    method: "POST",
    headers: { authorization: `Bearer ${token}` },
    body,
  });
}

function viewerTokenFromProtocols(request: Request): string | null {
  const protocols = request.headers
    .get("sec-websocket-protocol")
    ?.split(",")
    .map((value) => value.trim());
  if (!protocols?.includes(RELAY_SUBPROTOCOL)) return null;
  return protocols.find((value) => value.startsWith("auth."))?.slice(5) ?? null;
}

async function connectViewer(request: Request, env: Env, sessionId: string): Promise<Response> {
  if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
    return json({ error: "websocket_required" }, { status: 426 });
  }
  const token = viewerTokenFromProtocols(request);
  if (!token) return json({ error: "unauthorized" }, { status: 401 });

  const headers = new Headers(request.headers);
  headers.set("x-viewer-token", token);
  return sessionStub(env, sessionId).fetch("https://session.internal/view", {
    headers,
  });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (request.method === "GET" && url.pathname === "/health") {
      return json({ ok: true });
    }
    if (request.method === "POST" && url.pathname === "/v1/sessions") {
      return createSession(request, env);
    }

    const match = url.pathname.match(/^\/v1\/sessions\/([A-Za-z0-9_-]+)\/(snapshot|view)$/);
    if (!match) return json({ error: "not_found" }, { status: 404 });
    const [, sessionId, action] = match;
    if (action === "snapshot" && request.method === "POST") {
      return publishSnapshot(request, env, sessionId);
    }
    if (action === "view" && request.method === "GET") {
      return connectViewer(request, env, sessionId);
    }
    return json({ error: "method_not_allowed" }, { status: 405 });
  },
};

export class GameSession {
  constructor(
    private readonly state: DurableObjectState,
    _env: Env,
  ) {}

  private async metadata(): Promise<SessionMetadata | undefined> {
    return this.state.storage.get<SessionMetadata>("metadata");
  }

  private async authorized(token: string, role: "producer" | "viewer"): Promise<boolean> {
    const metadata = await this.metadata();
    if (!metadata || metadata.expiresAt <= Date.now()) return false;
    const expected = role === "producer" ? metadata.producerTokenHash : metadata.viewerTokenHash;
    return (await hashToken(token)) === expected;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/init" && request.method === "POST") {
      if (await this.metadata()) return json({ error: "already_initialized" }, { status: 409 });
      const metadata = (await request.json()) as SessionMetadata;
      await this.state.storage.put("metadata", metadata);
      await this.state.storage.setAlarm(metadata.expiresAt);
      return json({ ok: true });
    }

    if (url.pathname === "/snapshot" && request.method === "POST") {
      const token = bearerToken(request);
      if (!token || !(await this.authorized(token, "producer"))) {
        return json({ error: "unauthorized" }, { status: 401 });
      }
      const message: RelayMessage = { type: "snapshot", snapshot: await request.json() };
      const encoded = JSON.stringify(message);
      for (const socket of this.state.getWebSockets("viewer")) socket.send(encoded);
      return json({ delivered: this.state.getWebSockets("viewer").length });
    }

    if (url.pathname === "/view") {
      const token = request.headers.get("x-viewer-token");
      if (!token || !(await this.authorized(token, "viewer"))) {
        return json({ error: "unauthorized" }, { status: 401 });
      }
      const pair = new WebSocketPair();
      const [client, server] = Object.values(pair);
      this.state.acceptWebSocket(server, ["viewer"]);
      server.serializeAttachment({ role: "viewer" });
      server.send(JSON.stringify({ type: "status", status: "waiting" } satisfies RelayMessage));
      return new Response(null, {
        status: 101,
        webSocket: client,
        headers: { "sec-websocket-protocol": RELAY_SUBPROTOCOL },
      });
    }

    return json({ error: "not_found" }, { status: 404 });
  }

  async alarm(): Promise<void> {
    for (const socket of this.state.getWebSockets()) socket.close(4001, "session expired");
    await this.state.storage.deleteAll();
  }
}
