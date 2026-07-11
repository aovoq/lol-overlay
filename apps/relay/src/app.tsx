import { isMobileSnapshot, type PairingSession, RELAY_SUBPROTOCOL } from "@lol-overlay/protocol";
import { Hono } from "hono";
import { cors } from "hono/cors";
import type { FC } from "hono/jsx";
import {
  bearerToken,
  CREATE_RATE_LIMIT,
  CREATE_RATE_WINDOW_MS,
  clientIp,
  createSecretFromRequest,
  hashToken,
  isDevRelayHost,
  MAX_SNAPSHOT_BYTES,
  PAIR_RATE_LIMIT,
  PAIR_RATE_WINDOW_MS,
  PAIRING_CODE_LIFETIME_MS,
  pairingCodeStub,
  randomPairingCode,
  randomToken,
  rateLimitStub,
  SESSION_LIFETIME_MS,
  sessionStub,
  timingSafeEqualString,
} from "./shared";
import type { Env, SessionMetadata } from "./types";

type AppEnv = { Bindings: Env };

async function enforceRateLimit(
  env: Env,
  request: Request,
  bucket: string,
  limit: number,
  windowMs: number,
): Promise<Response | null> {
  const response = await rateLimitStub(env).fetch("https://rate-limit.internal/hit", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ key: `${bucket}:${clientIp(request)}`, limit, windowMs }),
  });
  if (response.status === 429) return Response.json({ error: "rate_limited" }, { status: 429 });
  if (!response.ok) {
    return Response.json({ error: "rate_limit_unavailable" }, { status: 503 });
  }
  return null;
}

async function createSession(request: Request, env: Env): Promise<Response> {
  const configured = (env.MOBILE_RELAY_CREATE_SECRET ?? env.SESSION_CREATE_SECRET)?.trim();
  if (!configured && !isDevRelayHost(request)) {
    return Response.json({ error: "create_secret_required" }, { status: 503 });
  }
  if (configured) {
    const provided = createSecretFromRequest(request);
    if (!provided || !timingSafeEqualString(provided, configured)) {
      return Response.json({ error: "unauthorized" }, { status: 401 });
    }
  }
  const limited = await enforceRateLimit(
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
  if (!initialized.ok) {
    return Response.json({ error: "session_init_failed" }, { status: 502 });
  }

  const relayUrl = new URL(request.url).origin;
  const viewerUrl = new URL(env.MOBILE_APP_URL ?? "loloverlay://pair");
  viewerUrl.searchParams.set("relay", relayUrl);
  viewerUrl.searchParams.set("session", sessionId);
  viewerUrl.hash = new URLSearchParams({ token: viewerToken }).toString();

  const codeExpiresAt = Math.min(expiresAt, Date.now() + PAIRING_CODE_LIFETIME_MS);
  let pairingCode = "";
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const candidate = randomPairingCode();
    const initializedCode = await pairingCodeStub(env, candidate).fetch(
      "https://pairing-code.internal/init",
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
  if (!pairingCode) {
    return Response.json({ error: "pairing_code_unavailable" }, { status: 503 });
  }

  const response: PairingSession = {
    sessionId,
    producerToken,
    viewerUrl: viewerUrl.href,
    pairingCode,
    pairingCodeExpiresAt: codeExpiresAt,
    expiresAt,
  };
  return Response.json(response, { status: 201 });
}

async function claimPairingCode(request: Request, env: Env, code: string): Promise<Response> {
  const limited = await enforceRateLimit(
    env,
    request,
    "pair",
    PAIR_RATE_LIMIT,
    PAIR_RATE_WINDOW_MS,
  );
  if (limited) return limited;
  if (!/^\d{6}$/.test(code)) {
    return Response.json({ error: "invalid_code" }, { status: 400 });
  }
  return pairingCodeStub(env, code).fetch("https://pairing-code.internal/claim", {
    method: "POST",
  });
}

async function redeemPairingCode(request: Request, env: Env): Promise<Response> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return Response.json({ error: "invalid_json" }, { status: 400 });
  }
  const code = typeof body === "object" && body !== null && "code" in body ? String(body.code) : "";
  return claimPairingCode(request, env, code);
}

async function publishSnapshot(request: Request, env: Env, sessionId: string): Promise<Response> {
  const token = bearerToken(request);
  if (!token) return Response.json({ error: "unauthorized" }, { status: 401 });
  const body = await request.text();
  if (new TextEncoder().encode(body).byteLength > MAX_SNAPSHOT_BYTES) {
    return Response.json({ error: "snapshot_too_large" }, { status: 413 });
  }
  let snapshot: unknown;
  try {
    snapshot = JSON.parse(body);
  } catch {
    return Response.json({ error: "invalid_json" }, { status: 400 });
  }
  if (!isMobileSnapshot(snapshot)) {
    return Response.json({ error: "invalid_snapshot" }, { status: 400 });
  }
  return sessionStub(env, sessionId).fetch("https://session.internal/snapshot", {
    method: "POST",
    headers: { authorization: `Bearer ${token}` },
    body,
  });
}

async function revokeSession(request: Request, env: Env, sessionId: string): Promise<Response> {
  const token = bearerToken(request);
  if (!token) return Response.json({ error: "unauthorized" }, { status: 401 });
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
    return Response.json({ error: "websocket_required" }, { status: 426 });
  }
  const token = viewerTokenFromProtocols(request);
  if (!token) return Response.json({ error: "unauthorized" }, { status: 401 });
  const headers = new Headers(request.headers);
  headers.set("x-viewer-token", token);
  return sessionStub(env, sessionId).fetch("https://session.internal/view", { headers });
}

const styles = `
  :root { color-scheme: dark; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
  * { box-sizing: border-box; }
  body { margin: 0; background: #090a0d; color: #f6f7f9; }
  main { width: min(920px, calc(100% - 32px)); margin: 48px auto; }
  header { border-left: 3px solid #ff465d; padding-left: 16px; margin-bottom: 28px; }
  h1 { margin: 5px 0; font: 800 28px/1.2 system-ui; }
  .eyebrow, h2 { color: #ff7183; text-transform: uppercase; letter-spacing: .1em; font-size: 11px; }
  .muted { color: #9297a3; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(270px, 1fr)); gap: 14px; }
  section { background: #111318; border: 1px solid #2d3038; padding: 18px; border-radius: 5px; }
  code { color: #dfe2e8; overflow-wrap: anywhere; }
  form { display: flex; gap: 8px; margin-top: 14px; }
  input { min-width: 0; flex: 1; background: #090a0d; border: 1px solid #454954; color: white; padding: 10px; font: inherit; }
  button, .button { border: 0; border-radius: 3px; background: #ff465d; color: #090a0d; padding: 10px 14px; font-weight: 800; cursor: pointer; text-decoration: none; }
  ul { padding-left: 20px; line-height: 1.8; }
`;

const DebugLayout: FC<{ children: unknown }> = ({ children }) => (
  <html lang="ja">
    <head>
      <meta charSet="utf-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1" />
      <title>LoL Overlay Relay Debug</title>
      <style dangerouslySetInnerHTML={{ __html: styles }} />
    </head>
    <body>{children}</body>
  </html>
);

function DebugPage({ createProtected }: { createProtected: boolean }) {
  return (
    <DebugLayout>
      <main>
        <header>
          <div class="eyebrow">LoL Overlay</div>
          <h1>Relay Debug</h1>
          <div class="muted">Worker API、pairing、Expo Web viewerの動作確認</div>
        </header>
        <div class="grid">
          <section>
            <h2>Runtime</h2>
            <p>
              Health: <strong>OK</strong>
            </p>
            <p>
              Session creation: <strong>{createProtected ? "protected" : "local open"}</strong>
            </p>
            <p>
              <a class="button" href="/viewer/">
                Open Web Viewer
              </a>
            </p>
          </section>
          <section>
            <h2>Pair Web Viewer</h2>
            <p class="muted">Desktopに表示された6桁コードを一度だけ使用します。</p>
            <form action="/debug/viewer" method="get">
              <input
                name="code"
                inputMode="numeric"
                pattern="[0-9]{6}"
                maxLength={6}
                placeholder="000000"
                required
              />
              <button type="submit">OPEN</button>
            </form>
          </section>
          <section>
            <h2>Public endpoints</h2>
            <ul>
              <li>
                <code>POST /v1/sessions</code>
              </li>
              <li>
                <code>POST /v1/pair</code>
              </li>
              <li>
                <code>POST /v1/sessions/:id/snapshot</code>
              </li>
              <li>
                <code>GET /v1/sessions/:id/view</code>
              </li>
              <li>
                <code>DELETE /v1/sessions/:id</code>
              </li>
            </ul>
          </section>
        </div>
      </main>
    </DebugLayout>
  );
}

export function createApp(): Hono<AppEnv> {
  const app = new Hono<AppEnv>();
  app.use("/v1/*", async (c, next) => {
    await next();
    c.header("cache-control", "no-store");
  });
  app.use(
    "/v1/*",
    cors({
      origin: "*",
      allowHeaders: ["authorization", "content-type", "x-session-create-key"],
      allowMethods: ["GET", "POST", "DELETE", "OPTIONS"],
    }),
  );

  app.get("/health", (c) => c.json({ ok: true }));
  app.post("/v1/sessions", (c) => createSession(c.req.raw, c.env));
  app.post("/v1/pair", (c) => redeemPairingCode(c.req.raw, c.env));
  app.post("/v1/sessions/:sessionId/snapshot", (c) =>
    publishSnapshot(c.req.raw, c.env, c.req.param("sessionId")),
  );
  app.delete("/v1/sessions/:sessionId", (c) =>
    revokeSession(c.req.raw, c.env, c.req.param("sessionId")),
  );
  app.get("/v1/sessions/:sessionId/view", (c) =>
    connectViewer(c.req.raw, c.env, c.req.param("sessionId")),
  );
  app.all("/v1/*", (c) => c.json({ error: "method_not_allowed" }, 405));

  app.get("/debug", (c) =>
    c.html(
      <DebugPage
        createProtected={Boolean(
          (c.env.MOBILE_RELAY_CREATE_SECRET ?? c.env.SESSION_CREATE_SECRET)?.trim(),
        )}
      />,
    ),
  );
  app.get("/debug/viewer", async (c) => {
    const claimed = await claimPairingCode(c.req.raw, c.env, c.req.query("code") ?? "");
    if (!claimed.ok) return claimed;
    const value = (await claimed.json()) as { viewerUrl: string };
    const target = new URL("/viewer/", c.req.url);
    target.hash = new URLSearchParams({ pair: value.viewerUrl }).toString();
    return c.redirect(target.href, 302);
  });

  // Expo emits root-relative bundle URLs even when its index lives under /viewer.
  app.get("/_expo/*", (c) => {
    const assetUrl = new URL(c.req.url);
    assetUrl.pathname = `/viewer${assetUrl.pathname}`;
    return c.env.ASSETS.fetch(new Request(assetUrl, c.req.raw));
  });

  app.all("*", (c) => c.env.ASSETS.fetch(c.req.raw));
  return app;
}

export const app = createApp();
