import {
  isMobileSnapshot,
  type PairingSession,
  RELAY_SUBPROTOCOL,
  type RelayMessage,
} from "@lol-overlay/protocol";

interface Env {
  SESSIONS: DurableObjectNamespace;
  RATE_LIMITS: DurableObjectNamespace;
  MOBILE_APP_URL?: string;
  /** When set, POST /v1/sessions requires this shared secret. */
  SESSION_CREATE_SECRET?: string;
}

interface SessionMetadata {
  producerTokenHash: string;
  viewerTokenHash: string;
  expiresAt: number;
}

interface PairingCodeMetadata {
  viewerUrl: string;
  expiresAt: number;
}

const SESSION_LIFETIME_MS = 4 * 60 * 60 * 1000;
const PAIRING_CODE_LIFETIME_MS = 10 * 60 * 1000;
const MAX_SNAPSHOT_BYTES = 64 * 1024;
const CREATE_RATE_LIMIT = 20;
const CREATE_RATE_WINDOW_MS = 60 * 60 * 1000;
const PAIR_RATE_LIMIT = 10;
const PAIR_RATE_WINDOW_MS = 60 * 1000;
const TOKEN_HASH_RE = /^[a-f0-9]{64}$/;

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

function randomPairingCode(): string {
  const value = new Uint32Array(1);
  crypto.getRandomValues(value);
  return String((value[0] ?? 0) % 1_000_000).padStart(6, "0");
}

async function hashToken(token: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(token));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function timingSafeEqualString(left: string, right: string): boolean {
  if (left.length !== right.length) return false;
  const a = new TextEncoder().encode(left);
  const b = new TextEncoder().encode(right);
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) {
    diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  }
  return diff === 0;
}

function bearerToken(request: Request): string | null {
  const value = request.headers.get("authorization");
  return value?.startsWith("Bearer ") ? value.slice(7) : null;
}

function createSecretFromRequest(request: Request): string | null {
  return bearerToken(request) ?? request.headers.get("x-session-create-key");
}

function clientIp(request: Request): string {
  // Prefer the Cloudflare edge IP. Do not trust X-Forwarded-For for public limits.
  return request.headers.get("cf-connecting-ip") ?? "local";
}

function isDevRelayHost(request: Request): boolean {
  const host = new URL(request.url).hostname;
  return host === "127.0.0.1" || host === "localhost" || host === "[::1]";
}

function sessionStub(env: Env, sessionId: string): DurableObjectStub {
  return env.SESSIONS.get(env.SESSIONS.idFromName(sessionId));
}

function rateLimitStub(env: Env): DurableObjectStub {
  return env.RATE_LIMITS.get(env.RATE_LIMITS.idFromName("v1"));
}

function isSessionMetadata(value: unknown): value is SessionMetadata {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.producerTokenHash === "string" &&
    TOKEN_HASH_RE.test(record.producerTokenHash) &&
    typeof record.viewerTokenHash === "string" &&
    TOKEN_HASH_RE.test(record.viewerTokenHash) &&
    typeof record.expiresAt === "number" &&
    Number.isFinite(record.expiresAt) &&
    record.expiresAt > Date.now()
  );
}

async function enforceDurableRateLimit(
  env: Env,
  request: Request,
  bucket: string,
  limit: number,
  windowMs: number,
): Promise<Response | null> {
  const response = await rateLimitStub(env).fetch("https://rate-limit.internal/hit", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      key: `${bucket}:${clientIp(request)}`,
      limit,
      windowMs,
    }),
  });
  if (response.status === 429) return json({ error: "rate_limited" }, { status: 429 });
  if (!response.ok) return json({ error: "rate_limit_unavailable" }, { status: 503 });
  return null;
}

async function createSession(request: Request, env: Env): Promise<Response> {
  const configured = env.SESSION_CREATE_SECRET?.trim();
  if (!configured && !isDevRelayHost(request)) {
    return json({ error: "create_secret_required" }, { status: 503 });
  }
  if (configured) {
    const provided = createSecretFromRequest(request);
    if (!provided || !timingSafeEqualString(provided, configured)) {
      return json({ error: "unauthorized" }, { status: 401 });
    }
  }

  const limited = await enforceDurableRateLimit(
    env,
    request,
    "create",
    CREATE_RATE_LIMIT,
    CREATE_RATE_WINDOW_MS,
  );
  if (limited) return limited;

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

  let pairingCode = "";
  const codeExpiresAt = Math.min(expiresAt, Date.now() + PAIRING_CODE_LIFETIME_MS);
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const candidate = randomPairingCode();
    const initializedCode = await sessionStub(env, `code:${candidate}`).fetch(
      "https://session.internal/code/init",
      {
        method: "POST",
        body: JSON.stringify({ viewerUrl: viewerUrl.href, expiresAt: codeExpiresAt }),
      },
    );
    if (initializedCode.ok) {
      pairingCode = candidate;
      break;
    }
  }
  if (!pairingCode) return json({ error: "pairing_code_unavailable" }, { status: 503 });

  const response: PairingSession = {
    sessionId,
    producerToken,
    viewerUrl: viewerUrl.href,
    pairingCode,
    pairingCodeExpiresAt: codeExpiresAt,
    expiresAt,
  };
  return json(response, { status: 201 });
}

async function redeemPairingCode(request: Request, env: Env): Promise<Response> {
  const limited = await enforceDurableRateLimit(
    env,
    request,
    "pair",
    PAIR_RATE_LIMIT,
    PAIR_RATE_WINDOW_MS,
  );
  if (limited) return limited;

  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return json({ error: "invalid_json" }, { status: 400 });
  }
  const code = typeof body === "object" && body !== null && "code" in body ? String(body.code) : "";
  if (!/^\d{6}$/.test(code)) return json({ error: "invalid_code" }, { status: 400 });
  return sessionStub(env, `code:${code}`).fetch("https://session.internal/code/claim", {
    method: "POST",
  });
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

async function revokeSession(request: Request, env: Env, sessionId: string): Promise<Response> {
  const token = bearerToken(request);
  if (!token) return json({ error: "unauthorized" }, { status: 401 });
  return sessionStub(env, sessionId).fetch("https://session.internal/revoke", {
    method: "POST",
    headers: { authorization: `Bearer ${token}` },
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
    if (request.method === "POST" && url.pathname === "/v1/pair") {
      return redeemPairingCode(request, env);
    }

    const match = url.pathname.match(/^\/v1\/sessions\/([A-Za-z0-9_-]+)(?:\/(snapshot|view))?$/);
    if (!match) return json({ error: "not_found" }, { status: 404 });
    const [, sessionId, action] = match;

    if (!action && request.method === "DELETE") {
      return revokeSession(request, env, sessionId);
    }
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
    return timingSafeEqualString(await hashToken(token), expected);
  }

  private async shutDown(code: number, reason: string): Promise<void> {
    for (const socket of this.state.getWebSockets()) {
      try {
        socket.close(code, reason);
      } catch {
        // Socket may already be closing.
      }
    }
    await this.state.storage.deleteAll();
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/code/init" && request.method === "POST") {
      if (await this.state.storage.get("pairingCode")) {
        return json({ error: "already_initialized" }, { status: 409 });
      }
      let value: Partial<PairingCodeMetadata>;
      try {
        value = (await request.json()) as Partial<PairingCodeMetadata>;
      } catch {
        return json({ error: "invalid_json" }, { status: 400 });
      }
      if (
        typeof value.viewerUrl !== "string" ||
        typeof value.expiresAt !== "number" ||
        value.expiresAt <= Date.now()
      ) {
        return json({ error: "invalid_metadata" }, { status: 400 });
      }
      const metadata: PairingCodeMetadata = {
        viewerUrl: value.viewerUrl,
        expiresAt: value.expiresAt,
      };
      await this.state.storage.put("pairingCode", metadata);
      await this.state.storage.setAlarm(metadata.expiresAt);
      return json({ ok: true });
    }

    if (url.pathname === "/code/claim" && request.method === "POST") {
      const value = await this.state.storage.get<PairingCodeMetadata>("pairingCode");
      if (!value || value.expiresAt <= Date.now()) {
        return json({ error: "invalid_code" }, { status: 404 });
      }
      await this.state.storage.deleteAll();
      return json({ viewerUrl: value.viewerUrl });
    }

    if (url.pathname === "/init" && request.method === "POST") {
      if (await this.metadata()) return json({ error: "already_initialized" }, { status: 409 });
      let metadata: unknown;
      try {
        metadata = await request.json();
      } catch {
        return json({ error: "invalid_json" }, { status: 400 });
      }
      if (!isSessionMetadata(metadata)) {
        return json({ error: "invalid_metadata" }, { status: 400 });
      }
      await this.state.storage.put("metadata", metadata);
      await this.state.storage.setAlarm(metadata.expiresAt);
      return json({ ok: true });
    }

    if (url.pathname === "/snapshot" && request.method === "POST") {
      const token = bearerToken(request);
      if (!token || !(await this.authorized(token, "producer"))) {
        return json({ error: "unauthorized" }, { status: 401 });
      }
      const snapshot = await request.json();
      if (!isMobileSnapshot(snapshot)) {
        return json({ error: "invalid_snapshot" }, { status: 400 });
      }
      await this.state.storage.put("lastSnapshot", snapshot);
      const message: RelayMessage = { type: "snapshot", snapshot };
      const encoded = JSON.stringify(message);
      for (const socket of this.state.getWebSockets("viewer")) socket.send(encoded);
      return json({ delivered: this.state.getWebSockets("viewer").length });
    }

    if (url.pathname === "/revoke" && request.method === "POST") {
      const token = bearerToken(request);
      if (!token || !(await this.authorized(token, "producer"))) {
        return json({ error: "unauthorized" }, { status: 401 });
      }
      await this.shutDown(4002, "session revoked");
      return json({ ok: true });
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

      const lastSnapshot = await this.state.storage.get("lastSnapshot");
      if (lastSnapshot !== undefined && isMobileSnapshot(lastSnapshot)) {
        server.send(
          JSON.stringify({ type: "snapshot", snapshot: lastSnapshot } satisfies RelayMessage),
        );
      } else {
        server.send(JSON.stringify({ type: "status", status: "waiting" } satisfies RelayMessage));
      }
      return new Response(null, {
        status: 101,
        webSocket: client,
        headers: { "sec-websocket-protocol": RELAY_SUBPROTOCOL },
      });
    }

    return json({ error: "not_found" }, { status: 404 });
  }

  async alarm(): Promise<void> {
    await this.shutDown(4001, "session expired");
  }
}

export class RateLimit {
  constructor(private readonly state: DurableObjectState) {}

  async fetch(request: Request): Promise<Response> {
    if (request.method !== "POST") {
      return json({ error: "method_not_allowed" }, { status: 405 });
    }
    let body: unknown;
    try {
      body = await request.json();
    } catch {
      return json({ error: "invalid_json" }, { status: 400 });
    }
    if (typeof body !== "object" || body === null || Array.isArray(body)) {
      return json({ error: "invalid_body" }, { status: 400 });
    }
    const record = body as Record<string, unknown>;
    const key = typeof record.key === "string" ? record.key : "";
    const limit = typeof record.limit === "number" ? record.limit : NaN;
    const windowMs = typeof record.windowMs === "number" ? record.windowMs : NaN;
    if (
      !key ||
      !Number.isFinite(limit) ||
      limit < 1 ||
      !Number.isFinite(windowMs) ||
      windowMs < 1
    ) {
      return json({ error: "invalid_body" }, { status: 400 });
    }

    const now = Date.now();
    const current = await this.state.storage.get<{ count: number; resetAt: number }>(key);
    if (!current || current.resetAt <= now) {
      const next = { count: 1, resetAt: now + windowMs };
      await this.state.storage.put(key, next);
      await this.state.storage.setAlarm(next.resetAt);
      return json({ ok: true, count: 1 });
    }
    if (current.count >= limit) {
      return json({ error: "rate_limited" }, { status: 429 });
    }
    current.count += 1;
    await this.state.storage.put(key, current);
    return json({ ok: true, count: current.count });
  }

  async alarm(): Promise<void> {
    const now = Date.now();
    const entries = await this.state.storage.list<{ count: number; resetAt: number }>();
    const deletions: string[] = [];
    let nextAlarm = Number.POSITIVE_INFINITY;
    for (const [key, value] of entries) {
      if (value.resetAt <= now) deletions.push(key);
      else nextAlarm = Math.min(nextAlarm, value.resetAt);
    }
    if (deletions.length > 0) await this.state.storage.delete(deletions);
    if (Number.isFinite(nextAlarm)) await this.state.storage.setAlarm(nextAlarm);
  }
}
