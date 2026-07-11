import { describe, expect, it } from "vitest";
import { createApp } from "../src/app";
import type { Env } from "../src/types";

function namespace(handler: (request: Request, name: string) => Response | Promise<Response>) {
  return {
    idFromName: (name: string) => ({ name }),
    get: (id: { name: string }) => ({
      fetch: (input: RequestInfo | URL, init?: RequestInit) =>
        handler(new Request(input, init), id.name),
    }),
  } as unknown as DurableObjectNamespace;
}

function testEnv(overrides: Partial<Env> = {}): Env {
  return {
    SESSIONS: namespace(() => Response.json({ ok: true })),
    PAIRING_CODES: namespace(() => Response.json({ ok: true })),
    RATE_LIMITS: namespace(() => Response.json({ ok: true, count: 1 })),
    ASSETS: { fetch: () => new Response("asset") },
    MOBILE_APP_URL: "loloverlay://pair",
    MOBILE_RELAY_CREATE_SECRET: "create-secret",
    ...overrides,
  } as Env;
}

describe("relay app", () => {
  it("serves health and debug UI", async () => {
    const app = createApp();
    const health = await app.request("https://relay.example/health", undefined, testEnv());
    expect(await health.json()).toEqual({ ok: true });

    const debug = await app.request("https://relay.example/debug", undefined, testEnv());
    expect(debug.status).toBe(200);
    expect(await debug.text()).toContain("Relay Debug");
  });

  it("protects session creation", async () => {
    const response = await createApp().request(
      "https://relay.example/v1/sessions",
      { method: "POST" },
      testEnv(),
    );
    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({ error: "unauthorized" });
  });

  it("creates a session and a dedicated pairing code", async () => {
    const calls: Array<{ kind: string; name: string; path: string }> = [];
    const env = testEnv({
      SESSIONS: namespace((request, name) => {
        calls.push({ kind: "session", name, path: new URL(request.url).pathname });
        return Response.json({ ok: true });
      }),
      PAIRING_CODES: namespace((request, name) => {
        calls.push({ kind: "code", name, path: new URL(request.url).pathname });
        return Response.json({ ok: true });
      }),
    });
    const response = await createApp().request(
      "https://relay.example/v1/sessions",
      { method: "POST", headers: { authorization: "Bearer create-secret" } },
      env,
    );
    expect(response.status).toBe(201);
    const value = (await response.json()) as Record<string, unknown>;
    expect(value.sessionId).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(value.pairingCode).toMatch(/^\d{6}$/);
    expect(value.viewerUrl).toContain("relay=https%3A%2F%2Frelay.example");
    expect(calls.map(({ kind, path }) => `${kind}:${path}`)).toEqual([
      "session:/init",
      "code:/init",
    ]);
  });

  it("validates pairing and snapshots at the public boundary", async () => {
    const app = createApp();
    const invalidCode = await app.request(
      "https://relay.example/v1/pair",
      { method: "POST", body: JSON.stringify({ code: "123" }) },
      testEnv(),
    );
    expect(invalidCode.status).toBe(400);

    const invalidSnapshot = await app.request(
      "https://relay.example/v1/sessions/session/snapshot",
      {
        method: "POST",
        headers: { authorization: "Bearer producer" },
        body: JSON.stringify({ protocolVersion: 999 }),
      },
      testEnv(),
    );
    expect(invalidSnapshot.status).toBe(400);
    expect(await invalidSnapshot.json()).toEqual({ error: "invalid_snapshot" });
  });

  it("redeems a code into the browser viewer without putting the token in a query", async () => {
    const viewerUrl =
      "loloverlay://pair?relay=https%3A%2F%2Frelay.example&session=session#token=viewer";
    const env = testEnv({
      PAIRING_CODES: namespace(() => Response.json({ viewerUrl })),
    });
    const response = await createApp().request(
      "https://relay.example/debug/viewer?code=123456",
      undefined,
      env,
    );
    expect(response.status).toBe(302);
    const location = response.headers.get("location") ?? "";
    expect(location).toContain("https://relay.example/viewer/#pair=");
    expect(new URL(location).search).toBe("");
  });
});
