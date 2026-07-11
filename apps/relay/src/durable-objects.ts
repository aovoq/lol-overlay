import { isMobileSnapshot, type RelayMessage } from "@lol-overlay/protocol";
import { bearerToken, hashToken, isSessionMetadata, timingSafeEqualString } from "./shared";
import type { Env, PairingCodeMetadata, SessionMetadata } from "./types";

function json(data: unknown, init: ResponseInit = {}): Response {
  const headers = new Headers(init.headers);
  headers.set("content-type", "application/json; charset=utf-8");
  headers.set("cache-control", "no-store");
  return new Response(JSON.stringify(data), { ...init, headers });
}

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

  private async initialize(request: Request): Promise<Response> {
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

  private async publishSnapshot(request: Request): Promise<Response> {
    const token = bearerToken(request);
    if (!token || !(await this.authorized(token, "producer"))) {
      return json({ error: "unauthorized" }, { status: 401 });
    }
    const snapshot = await request.json();
    if (!isMobileSnapshot(snapshot)) {
      return json({ error: "invalid_snapshot" }, { status: 400 });
    }
    await this.state.storage.put("lastSnapshot", snapshot);
    const encoded = JSON.stringify({ type: "snapshot", snapshot } satisfies RelayMessage);
    const viewers = this.state.getWebSockets("viewer");
    for (const socket of viewers) socket.send(encoded);
    return json({ delivered: viewers.length });
  }

  private async revoke(request: Request): Promise<Response> {
    const token = bearerToken(request);
    if (!token || !(await this.authorized(token, "producer"))) {
      return json({ error: "unauthorized" }, { status: 401 });
    }
    await this.shutDown(4002, "session revoked");
    return json({ ok: true });
  }

  private async connectViewer(request: Request): Promise<Response> {
    const token = request.headers.get("x-viewer-token");
    if (!token || !(await this.authorized(token, "viewer"))) {
      return json({ error: "unauthorized" }, { status: 401 });
    }
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.state.acceptWebSocket(server, ["viewer"]);
    server.serializeAttachment({ role: "viewer" });

    const lastSnapshot = await this.state.storage.get("lastSnapshot");
    const message: RelayMessage =
      lastSnapshot !== undefined && isMobileSnapshot(lastSnapshot)
        ? { type: "snapshot", snapshot: lastSnapshot }
        : { type: "status", status: "waiting" };
    server.send(JSON.stringify(message));
    return new Response(null, {
      status: 101,
      webSocket: client,
      headers: { "sec-websocket-protocol": "lol-overlay-v1" },
    });
  }

  async fetch(request: Request): Promise<Response> {
    const key = `${request.method} ${new URL(request.url).pathname}`;
    switch (key) {
      case "POST /init":
        return this.initialize(request);
      case "POST /snapshot":
        return this.publishSnapshot(request);
      case "POST /revoke":
        return this.revoke(request);
      case "GET /view":
        return this.connectViewer(request);
      default:
        return json({ error: "not_found" }, { status: 404 });
    }
  }

  async alarm(): Promise<void> {
    await this.shutDown(4001, "session expired");
  }
}

export class PairingCode {
  constructor(private readonly state: DurableObjectState) {}

  private async initialize(request: Request): Promise<Response> {
    if (await this.state.storage.get("metadata")) {
      return json({ error: "already_initialized" }, { status: 409 });
    }
    let value: unknown;
    try {
      value = await request.json();
    } catch {
      return json({ error: "invalid_json" }, { status: 400 });
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
      return json({ error: "invalid_metadata" }, { status: 400 });
    }
    const record = value as Record<string, unknown>;
    if (
      typeof record.viewerUrl !== "string" ||
      typeof record.expiresAt !== "number" ||
      record.expiresAt <= Date.now()
    ) {
      return json({ error: "invalid_metadata" }, { status: 400 });
    }
    const metadata: PairingCodeMetadata = {
      viewerUrl: record.viewerUrl,
      expiresAt: record.expiresAt,
    };
    await this.state.storage.put("metadata", metadata);
    await this.state.storage.setAlarm(metadata.expiresAt);
    return json({ ok: true });
  }

  private async claim(): Promise<Response> {
    const value = await this.state.storage.get<PairingCodeMetadata>("metadata");
    if (!value || value.expiresAt <= Date.now()) {
      return json({ error: "invalid_code" }, { status: 404 });
    }
    await this.state.storage.deleteAll();
    return json({ viewerUrl: value.viewerUrl });
  }

  async fetch(request: Request): Promise<Response> {
    const key = `${request.method} ${new URL(request.url).pathname}`;
    if (key === "POST /init") return this.initialize(request);
    if (key === "POST /claim") return this.claim();
    return json({ error: "not_found" }, { status: 404 });
  }

  async alarm(): Promise<void> {
    await this.state.storage.deleteAll();
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
    const limit = typeof record.limit === "number" ? record.limit : Number.NaN;
    const windowMs = typeof record.windowMs === "number" ? record.windowMs : Number.NaN;
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
