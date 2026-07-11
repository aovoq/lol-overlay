export const PROTOCOL_VERSION = 1 as const;
export const RELAY_SUBPROTOCOL = "lol-overlay-v1";

export interface MobileEnemy {
  name: string;
  rawName: string;
  position: string;
  items: number[];
}

export interface MobileItemRecommendation {
  itemId: number;
  name: string;
  score: number;
  reason: string;
}

export interface MobileSkillOrder {
  maxOrder: number[];
  levelOrder: number[];
  winRate: number;
  games: number;
}

export interface MobileThreatProfile {
  adCount: number;
  apCount: number;
  tankCount: number;
  ccHeavy: boolean;
}

export interface MobileGame {
  gameMode: string;
  gameTime: number;
  selfChampion: string;
  selfRawName: string;
  selfPosition: string;
  allies: string[];
  enemies: MobileEnemy[];
  threats: MobileThreatProfile;
  skillOrder: MobileSkillOrder | null;
  items: MobileItemRecommendation[];
}

export interface MobileSnapshot {
  protocolVersion: typeof PROTOCOL_VERSION;
  sequence: number;
  capturedAt: number;
  phase: string;
  clientUp: boolean;
  game: MobileGame | null;
}

export interface PairingSession {
  sessionId: string;
  producerToken: string;
  viewerUrl: string;
  pairingCode: string;
  pairingCodeExpiresAt: number;
  expiresAt: number;
}

export type RelayMessage =
  | { type: "snapshot"; snapshot: MobileSnapshot }
  | { type: "status"; status: "connected" | "waiting" }
  | { type: "error"; code: string; message: string };

export interface PairingLink {
  relayUrl: string;
  sessionId: string;
  viewerToken: string;
}

export function normalizePairingCode(value: string): string | null {
  const code = value.replace(/\D/g, "");
  return code.length === 6 ? code : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNumberArray(value: unknown): value is number[] {
  return Array.isArray(value) && value.every(isFiniteNumber);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isMobileEnemy(value: unknown): value is MobileEnemy {
  return (
    isRecord(value) &&
    typeof value.name === "string" &&
    typeof value.rawName === "string" &&
    typeof value.position === "string" &&
    isNumberArray(value.items)
  );
}

function isMobileItemRecommendation(value: unknown): value is MobileItemRecommendation {
  return (
    isRecord(value) &&
    isFiniteNumber(value.itemId) &&
    typeof value.name === "string" &&
    isFiniteNumber(value.score) &&
    typeof value.reason === "string"
  );
}

function isMobileSkillOrder(value: unknown): value is MobileSkillOrder {
  return (
    isRecord(value) &&
    isNumberArray(value.maxOrder) &&
    isNumberArray(value.levelOrder) &&
    isFiniteNumber(value.winRate) &&
    isFiniteNumber(value.games)
  );
}

function isMobileThreatProfile(value: unknown): value is MobileThreatProfile {
  return (
    isRecord(value) &&
    isFiniteNumber(value.adCount) &&
    isFiniteNumber(value.apCount) &&
    isFiniteNumber(value.tankCount) &&
    typeof value.ccHeavy === "boolean"
  );
}

function isMobileGame(value: unknown): value is MobileGame {
  return (
    isRecord(value) &&
    typeof value.gameMode === "string" &&
    isFiniteNumber(value.gameTime) &&
    typeof value.selfChampion === "string" &&
    typeof value.selfRawName === "string" &&
    typeof value.selfPosition === "string" &&
    isStringArray(value.allies) &&
    Array.isArray(value.enemies) &&
    value.enemies.every(isMobileEnemy) &&
    isMobileThreatProfile(value.threats) &&
    (value.skillOrder === null || isMobileSkillOrder(value.skillOrder)) &&
    Array.isArray(value.items) &&
    value.items.every(isMobileItemRecommendation)
  );
}

export function isMobileSnapshot(value: unknown): value is MobileSnapshot {
  if (!isRecord(value)) return false;
  if (
    value.protocolVersion !== PROTOCOL_VERSION ||
    !isFiniteNumber(value.sequence) ||
    !isFiniteNumber(value.capturedAt) ||
    typeof value.phase !== "string" ||
    typeof value.clientUp !== "boolean"
  ) {
    return false;
  }

  return value.game === null || isMobileGame(value.game);
}

export function isRelayMessage(value: unknown): value is RelayMessage {
  if (!isRecord(value) || typeof value.type !== "string") return false;
  if (value.type === "snapshot") return isMobileSnapshot(value.snapshot);
  if (value.type === "status") return value.status === "connected" || value.status === "waiting";
  return (
    value.type === "error" && typeof value.code === "string" && typeof value.message === "string"
  );
}

export function parsePairingLink(rawUrl: string): PairingLink | null {
  try {
    const url = new URL(rawUrl);
    const relayUrl = url.searchParams.get("relay");
    const sessionId = url.searchParams.get("session");
    const viewerToken = new URLSearchParams(url.hash.slice(1)).get("token");
    if (
      !relayUrl ||
      !sessionId ||
      !viewerToken ||
      !/^[A-Za-z0-9_-]+$/.test(sessionId) ||
      !/^[A-Za-z0-9_-]+$/.test(viewerToken)
    ) {
      return null;
    }
    const relay = new URL(relayUrl);
    if (relay.protocol === "http:") {
      if (!["127.0.0.1", "localhost", "[::1]"].includes(relay.hostname)) return null;
    } else if (relay.protocol !== "https:") {
      return null;
    }
    return {
      relayUrl: relay.href.replace(/\/$/, ""),
      sessionId,
      viewerToken,
    };
  } catch {
    return null;
  }
}

export function viewerWebSocketUrl(link: PairingLink): string {
  const url = new URL(`/v1/sessions/${encodeURIComponent(link.sessionId)}/view`, link.relayUrl);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}
