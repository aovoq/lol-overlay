import type { Context } from "hono";
import type { Env, SessionMetadata } from "./types";

export const SESSION_LIFETIME_MS = 4 * 60 * 60 * 1000;
export const PAIRING_CODE_LIFETIME_MS = 10 * 60 * 1000;
export const MAX_SNAPSHOT_BYTES = 64 * 1024;
export const CREATE_RATE_LIMIT = 20;
export const CREATE_RATE_WINDOW_MS = 60 * 60 * 1000;
export const PAIR_RATE_LIMIT = 10;
export const PAIR_RATE_WINDOW_MS = 60 * 1000;

const TOKEN_HASH_RE = /^[a-f0-9]{64}$/;

export function randomToken(bytes = 32): string {
  const value = new Uint8Array(bytes);
  crypto.getRandomValues(value);
  return btoa(String.fromCharCode(...value))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}

export function randomPairingCode(): string {
  const value = new Uint32Array(1);
  crypto.getRandomValues(value);
  return String((value[0] ?? 0) % 1_000_000).padStart(6, "0");
}

export async function hashToken(token: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(token));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function timingSafeEqualString(left: string, right: string): boolean {
  if (left.length !== right.length) return false;
  const a = new TextEncoder().encode(left);
  const b = new TextEncoder().encode(right);
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return diff === 0;
}

export function bearerToken(request: Request): string | null {
  const value = request.headers.get("authorization");
  return value?.startsWith("Bearer ") ? value.slice(7) : null;
}

export function createSecretFromRequest(request: Request): string | null {
  return bearerToken(request) ?? request.headers.get("x-session-create-key");
}

export function clientIp(request: Request): string {
  return request.headers.get("cf-connecting-ip") ?? "local";
}

export function isDevRelayHost(request: Request): boolean {
  const host = new URL(request.url).hostname;
  return host === "127.0.0.1" || host === "localhost" || host === "[::1]";
}

export function isSessionMetadata(value: unknown): value is SessionMetadata {
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

export function sessionStub(env: Env, sessionId: string): DurableObjectStub {
  return env.SESSIONS.get(env.SESSIONS.idFromName(sessionId));
}

export function pairingCodeStub(env: Env, code: string): DurableObjectStub {
  return env.PAIRING_CODES.get(env.PAIRING_CODES.idFromName(code));
}

export function rateLimitStub(env: Env): DurableObjectStub {
  return env.RATE_LIMITS.get(env.RATE_LIMITS.idFromName("v1"));
}

export function errorJson(c: Context, error: string, status: number): Response {
  return c.json({ error }, status as 400);
}
