import { describe, expect, it } from "vitest";
import {
  isMobileCommand,
  isMobileSnapshot,
  isRelayMessage,
  normalizePairingCode,
  parsePairingLink,
  viewerWebSocketUrl,
} from ".";

const snapshot = {
  protocolVersion: 1,
  sequence: 1,
  capturedAt: 1,
  phase: "InProgress",
  clientUp: true,
  matchmaking: null,
  game: {
    gameMode: "CLASSIC",
    gameTime: 42,
    selfChampion: "Talon",
    selfRawName: "Talon",
    selfPosition: "MIDDLE",
    allies: ["Ashe"],
    enemies: [{ name: "Ahri", rawName: "Ahri", position: "MIDDLE", items: [1056] }],
    threats: { adCount: 2, apCount: 2, tankCount: 1, ccHeavy: false },
    skillOrder: { maxOrder: [1, 2, 3], levelOrder: [1, 2], winRate: 51.2, games: 300 },
    items: [{ itemId: 3142, name: "Youmuu", score: 1, reason: "damage" }],
  },
} as const;

describe("pairing links", () => {
  it("normalizes a six-digit pairing code", () => {
    expect(normalizePairingCode("123 456")).toBe("123456");
    expect(normalizePairingCode("12345")).toBeNull();
  });
  it("parses the relay, session, and fragment token", () => {
    expect(
      parsePairingLink(
        "loloverlay://pair?relay=https%3A%2F%2Frelay.example.com&session=abc#token=secret",
      ),
    ).toEqual({
      relayUrl: "https://relay.example.com",
      sessionId: "abc",
      viewerToken: "secret",
    });
  });

  it("builds a secure websocket URL", () => {
    expect(
      viewerWebSocketUrl({
        relayUrl: "https://relay.example.com",
        sessionId: "abc",
        viewerToken: "secret",
      }),
    ).toBe("wss://relay.example.com/v1/sessions/abc/view");
  });

  it.each([
    "loloverlay://pair?relay=file%3A%2F%2Ftmp&session=abc#token=secret",
    "loloverlay://pair?relay=https%3A%2F%2Frelay.example.com&session=../abc#token=secret",
    "loloverlay://pair?relay=https%3A%2F%2Frelay.example.com&session=abc#token=bad%20token",
    "loloverlay://pair?relay=http%3A%2F%2Fevil.example.com&session=abc#token=secret",
  ])("rejects unsafe connection fields", (url) => {
    expect(parsePairingLink(url)).toBeNull();
  });

  it("allows localhost http relays for local development", () => {
    expect(
      parsePairingLink(
        "loloverlay://pair?relay=http%3A%2F%2F127.0.0.1%3A8787&session=abc#token=secret",
      ),
    ).toEqual({
      relayUrl: "http://127.0.0.1:8787",
      sessionId: "abc",
      viewerToken: "secret",
    });
  });
});

describe("relay payload validation", () => {
  it("accepts a complete snapshot and relay envelope", () => {
    expect(isMobileSnapshot(snapshot)).toBe(true);
    expect(isRelayMessage({ type: "snapshot", snapshot })).toBe(true);
  });

  it.each([
    { ...snapshot, game: { ...snapshot.game, threats: null } },
    { ...snapshot, game: { ...snapshot.game, enemies: [null] } },
    { ...snapshot, game: { ...snapshot.game, skillOrder: {} } },
    { ...snapshot, game: { ...snapshot.game, items: [{ itemId: "3142" }] } },
  ])("rejects malformed nested game data", (value) => {
    expect(isMobileSnapshot(value)).toBe(false);
  });

  it("rejects an invalid relay envelope", () => {
    expect(isRelayMessage({ type: "error", message: "broken" })).toBe(false);
  });

  it("accepts only bounded ready-check commands", () => {
    expect(
      isMobileCommand({ type: "readyCheckResponse", requestId: "one", response: "accept" }),
    ).toBe(true);
    expect(isMobileCommand({ type: "readyCheckResponse", requestId: "", response: "accept" })).toBe(
      false,
    );
    expect(
      isMobileCommand({ type: "readyCheckResponse", requestId: "one", response: "later" }),
    ).toBe(false);
  });
});
