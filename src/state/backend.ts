import { listen } from "@tauri-apps/api/event";
import { createSignal } from "solid-js";
import type {
  ChampSelectEvent,
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

const [selectedRole, setSelectedRole] = createSignal("middle");
const [activeTab, setActiveTab] = createSignal<"best" | "vs">("best");
const [vsEnemyId, setVsEnemyId] = createSignal(0);
const [userPickedVsEnemy, setUserPickedVsEnemy] = createSignal(false);
const [hoverChampId, setHoverChampId] = createSignal(0);
const [importState, setImportState] = createSignal<"idle" | "importing" | "imported" | "failed">(
  "idle",
);
const [vsMenuOpen, setVsMenuOpen] = createSignal(false);

export {
  activeTab,
  champSelect,
  hoverChampId,
  importState,
  interactive,
  lpChange,
  matchHistory,
  phase,
  recommendations,
  runeImported,
  selectedRole,
  setActiveTab,
  setHoverChampId,
  setImportState,
  setSelectedRole,
  setUserPickedVsEnemy,
  setVsEnemyId,
  setVsMenuOpen,
  summoner,
  userPickedVsEnemy,
  vsEnemyId,
  vsMenuOpen,
  windowMode,
};

listen<PhaseEvent>("phase", (e) => setPhase(e.payload)).catch(() => {});
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
  const payload = e.payload;
  setChampSelect(payload);

  const revealed = payload.enemyChampionIds.filter((id) => id > 0);

  if (!revealed.includes(vsEnemyId())) {
    setVsEnemyId(revealed[0] ?? 0);
    setUserPickedVsEnemy(false);
  } else if (!userPickedVsEnemy() && revealed.length > 0) {
    setVsEnemyId(revealed[0]);
  }

  if (!vsEnemyId() && activeTab() === "vs") setActiveTab("best");
  if (!payload.active) setHoverChampId(0);
}).catch(() => {});
