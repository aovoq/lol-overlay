import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import { retainDraft } from "../lib/draftRetention";
import type {
  AppSnapshot,
  ChampSelectEvent,
  GamePlayer,
  LogEvent,
  LpChangeEvent,
  PhaseEvent,
  RecentGame,
  RecommendationsEvent,
  RuneImportedEvent,
  SummonerEvent,
  WindowMode,
} from "../types";

const [phase, setPhase] = createSignal<PhaseEvent | null>(null);
const [recommendations, setRecommendations] = createSignal<RecommendationsEvent | null>(null);
const [runeImported, setRuneImported] = createSignal<RuneImportedEvent | null>(null);
const [summoner, setSummoner] = createSignal<SummonerEvent | null>(null);
const [matchHistory, setMatchHistory] = createSignal<RecentGame[] | null>(null);
const [lpChange, setLpChange] = createSignal<LpChangeEvent | null>(null);
const [windowMode, setWindowMode] = createSignal<WindowMode>("overlay");
const [interactive, setInteractive] = createSignal(false);
const [champSelect, setChampSelect] = createSignal<ChampSelectEvent | null>(null);
/** Last active champ-select session, retained through load screen + game. */
const [lastDraft, setLastDraft] = createSignal<ChampSelectEvent | null>(null);
/** Participants of the running game (null until the load screen). */
const [gamePlayers, setGamePlayers] = createSignal<GamePlayer[] | null>(null);

const [selectedRole, setSelectedRole] = createSignal("middle");
const [vsEnemyId, setVsEnemyId] = createSignal(0);
const [userPickedVsEnemy, setUserPickedVsEnemy] = createSignal(false);
const [importState, setImportState] = createSignal<"idle" | "importing" | "imported" | "failed">(
  "idle",
);

export {
  champSelect,
  gamePlayers,
  importState,
  interactive,
  lastDraft,
  lpChange,
  matchHistory,
  phase,
  recommendations,
  runeImported,
  selectedRole,
  setImportState,
  setSelectedRole,
  setUserPickedVsEnemy,
  setVsEnemyId,
  summoner,
  userPickedVsEnemy,
  vsEnemyId,
  windowMode,
};

function applyChampSelect(payload: ChampSelectEvent) {
  setChampSelect(payload);
  // Retain the last live session for the load screen / in-game draft view;
  // the inactive sentinel keeps whatever came before it. Matchup auto-follow
  // lives in DraftPage (it needs tier data), and clearing the retained draft
  // is phase-driven (see the "phase" listener below).
  if (payload.active) setLastDraft(payload);
}

/** Drop every bit of retained draft context (dodge / back in the lobby). */
function clearDraftContext() {
  if (lastDraft()) setLastDraft(null);
  if (gamePlayers()) setGamePlayers(null);
  setVsEnemyId(0);
  setUserPickedVsEnemy(false);
}

export function hydrateBackend(snapshot: AppSnapshot) {
  setPhase(snapshot.phase);
  setWindowMode(snapshot.windowMode);
  applyChampSelect(snapshot.champSelect);
}

export const backendReady = invoke<AppSnapshot>("get_app_snapshot")
  .then(hydrateBackend)
  .catch(() => {});

listen<PhaseEvent>("phase", (e) => {
  setPhase(e.payload);
  if (!retainDraft(e.payload)) clearDraftContext();
}).catch(() => {});
listen<GamePlayer[]>("game-players", (e) => {
  setGamePlayers(e.payload.length > 0 ? e.payload : null);
}).catch(() => {});
listen<RecommendationsEvent>("recommendations", (e) => setRecommendations(e.payload)).catch(
  () => {},
);
listen<RuneImportedEvent>("rune-imported", (e) => setRuneImported(e.payload)).catch(() => {});
listen<SummonerEvent | null>("summoner", (e) => setSummoner(e.payload)).catch(() => {});
listen<RecentGame[]>("match-history", (e) => setMatchHistory(e.payload)).catch(() => {});
listen<LpChangeEvent>("lp-change", (e) => setLpChange(e.payload)).catch(() => {});
listen<LogEvent>("log", (e) => console.log(`[${e.payload.level}] ${e.payload.message}`)).catch(
  () => {},
);
listen<WindowMode>("window-mode", (e) => setWindowMode(e.payload)).catch(() => {});
listen<boolean>("interactive", (e) => setInteractive(e.payload)).catch(() => {});

listen<ChampSelectEvent>("champ-select", (e) => {
  applyChampSelect(e.payload);
}).catch(() => {});
