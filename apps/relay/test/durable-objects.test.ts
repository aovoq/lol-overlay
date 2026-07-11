import { describe, expect, it, vi } from "vitest";
import { GameSession, PairingCode, RateLimit } from "../src/durable-objects";
import { hashToken } from "../src/shared";
import type { Env } from "../src/types";

class MemoryStorage {
  values = new Map<string, unknown>();
  alarm: number | null = null;

  get<T>(key: string): Promise<T | undefined> {
    return Promise.resolve(this.values.get(key) as T | undefined);
  }

  put(key: string, value: unknown): Promise<void> {
    this.values.set(key, value);
    return Promise.resolve();
  }

  delete(keys: string | string[]): Promise<boolean | number> {
    if (Array.isArray(keys)) {
      let deleted = 0;
      for (const key of keys) if (this.values.delete(key)) deleted += 1;
      return Promise.resolve(deleted);
    }
    return Promise.resolve(this.values.delete(keys));
  }

  deleteAll(): Promise<void> {
    this.values.clear();
    return Promise.resolve();
  }

  list<T>(): Promise<Map<string, T>> {
    return Promise.resolve(this.values as Map<string, T>);
  }

  setAlarm(value: number): Promise<void> {
    this.alarm = value;
    return Promise.resolve();
  }
}

function state(storage = new MemoryStorage(), sockets: WebSocket[] = []): DurableObjectState {
  return {
    storage,
    getWebSockets: () => sockets,
    acceptWebSocket: vi.fn(),
  } as unknown as DurableObjectState;
}

describe("GameSession", () => {
  it("authenticates, stores, and broadcasts producer snapshots", async () => {
    const storage = new MemoryStorage();
    const viewer = { send: vi.fn(), close: vi.fn() } as unknown as WebSocket;
    const object = new GameSession(state(storage, [viewer]), {} as Env);
    const producerToken = "producer-token";
    const initialized = await object.fetch(
      new Request("https://internal/init", {
        method: "POST",
        body: JSON.stringify({
          producerTokenHash: await hashToken(producerToken),
          viewerTokenHash: await hashToken("viewer-token"),
          expiresAt: Date.now() + 60_000,
        }),
      }),
    );
    expect(initialized.status).toBe(200);

    const snapshot = {
      protocolVersion: 1,
      sequence: 1,
      capturedAt: Date.now(),
      phase: "InProgress",
      clientUp: true,
      game: null,
    };
    const published = await object.fetch(
      new Request("https://internal/snapshot", {
        method: "POST",
        headers: { authorization: `Bearer ${producerToken}` },
        body: JSON.stringify(snapshot),
      }),
    );
    expect(await published.json()).toEqual({ delivered: 1 });
    expect(storage.values.get("lastSnapshot")).toEqual(snapshot);
    expect(viewer.send).toHaveBeenCalledWith(JSON.stringify({ type: "snapshot", snapshot }));

    const unauthorized = await object.fetch(
      new Request("https://internal/snapshot", {
        method: "POST",
        headers: { authorization: "Bearer wrong" },
        body: JSON.stringify(snapshot),
      }),
    );
    expect(unauthorized.status).toBe(401);
  });
});

describe("PairingCode", () => {
  it("is single-use and rejects initialization collisions", async () => {
    const storage = new MemoryStorage();
    const object = new PairingCode(state(storage));
    const metadata = { viewerUrl: "loloverlay://pair#token=value", expiresAt: Date.now() + 60_000 };

    const initialized = await object.fetch(
      new Request("https://internal/init", { method: "POST", body: JSON.stringify(metadata) }),
    );
    expect(initialized.status).toBe(200);
    expect(storage.alarm).toBe(metadata.expiresAt);

    const collision = await object.fetch(
      new Request("https://internal/init", { method: "POST", body: JSON.stringify(metadata) }),
    );
    expect(collision.status).toBe(409);

    const claimed = await object.fetch(new Request("https://internal/claim", { method: "POST" }));
    expect(await claimed.json()).toEqual({ viewerUrl: metadata.viewerUrl });

    const reused = await object.fetch(new Request("https://internal/claim", { method: "POST" }));
    expect(reused.status).toBe(404);
  });
});

describe("RateLimit", () => {
  it("enforces a fixed-window limit and clears expired buckets", async () => {
    const storage = new MemoryStorage();
    const object = new RateLimit(state(storage));
    const hit = () =>
      object.fetch(
        new Request("https://internal/hit", {
          method: "POST",
          body: JSON.stringify({ key: "pair:local", limit: 2, windowMs: 1_000 }),
        }),
      );

    expect((await hit()).status).toBe(200);
    expect((await hit()).status).toBe(200);
    expect((await hit()).status).toBe(429);

    storage.values.set("pair:local", { count: 2, resetAt: Date.now() - 1 });
    await object.alarm();
    expect(storage.values.has("pair:local")).toBe(false);
  });
});
