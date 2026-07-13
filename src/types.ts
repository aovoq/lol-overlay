// TypeScript mirrors of the Rust payload structs. Every struct serializes with
// `#[serde(rename_all = "camelCase")]`, so the field names here must match the
// Rust snake_case names 1:1 after camel-casing — keep both sides in sync.
// Sources: src-tauri/src/events.rs, src-tauri/src/provider/mod.rs,
// src-tauri/src/engine.rs (Settings).

// ---- backend → frontend events ----

export interface PhaseEvent {
  phase: string;
  clientUp: boolean;
  inGame: boolean;
}

export interface ItemRecommendation {
  itemId: number;
  name: string;
  score: number;
  reason: string;
}

export interface EnemyChampion {
  name: string;
  rawName: string;
  position: string;
  items: number[];
}

/** One participant of the running game (snapshot.rs::GamePlayer); payload of
 * the "game-players" event, available from the load screen onwards. */
export interface GamePlayer {
  /** Riot ID, e.g. "Faker#KR1" ("" when unavailable). */
  riotId: string;
  name: string;
  rawName: string;
  /** "TOP" | "JUNGLE" | "MIDDLE" | "BOTTOM" | "UTILITY" | "". */
  position: string;
  ally: boolean;
}

export interface ThreatProfile {
  adCount: number;
  apCount: number;
  tankCount: number;
  ccHeavy: boolean;
}

export interface SkillOrder {
  /** Basic-skill max priority. 1 = Q, 2 = W, 3 = E, 4 = R. */
  maxOrder: number[];
  /** Level-by-level order. 1 = Q, 2 = W, 3 = E, 4 = R. */
  levelOrder: number[];
  winRate: number;
  games: number;
}

export interface RecommendationsEvent {
  selfChampion: string;
  /** Data Dragon image id ("Chogath"), for the panel's champion icon. */
  selfRawName: string;
  selfPosition: string;
  enemies: EnemyChampion[];
  threats: ThreatProfile;
  skillOrder?: SkillOrder | null;
  items: ItemRecommendation[];
}

export interface RuneImportedEvent {
  championId: number;
  pageName: string;
}

/** Logged-in summoner + solo rank (lcu.rs::SummonerInfo). Null = client gone. */
export interface SummonerEvent {
  gameName: string;
  tagLine: string;
  level: number;
  profileIconId: number;
  /** "" when unranked. */
  soloTier: string;
  /** Roman numeral ("II"); "NA" for apex tiers. */
  soloDivision: string;
  soloLp: number;
  soloWins: number;
  soloLosses: number;
}

/** One game of the local match history (lcu.rs::RecentGame), newest first. */
export interface RecentGame {
  championId: number;
  win: boolean;
  kills: number;
  deaths: number;
  assists: number;
  queueId: number;
  /** Unix millis. */
  gameCreation: number;
}

/** Solo-queue result detected after a game (events.rs::LpChangeEvent). */
export interface LpChangeEvent {
  win: boolean;
  /** New LP minus old LP; ignore when rankChange is non-empty. */
  lpDelta: number;
  tier: string;
  division: string;
  lp: number;
  /** "promoted" | "demoted" | "" */
  rankChange: string;
}

export interface LogEvent {
  level: string;
  message: string;
}

/** Champ-select state for the OPENLOL panel (events.rs::ChampSelectEvent). */
export interface ChampSelectEvent {
  active: boolean;
  /** "top" | "jungle" | "middle" | "bottom" | "utility" | "" (unknown). */
  myRole: string;
  /** Hovered or locked champion (0 = none). See `myLocked`. */
  myChampionId: number;
  myLocked: boolean;
  /** 5 slots in cell order; 0 = not picked/revealed yet. */
  myTeamChampionIds: number[];
  enemyChampionIds: number[];
  myBans: number[];
  enemyBans: number[];
  /** "PLANNING" | "BAN_PICK" | "FINALIZATION" | "GAME_STARTING" | "". */
  timerPhase: string;
}

/** Payload of the "window-mode" event. */
export type WindowMode = "overlay" | "champselect" | "ingame";
export type PresentationMode = "overlay" | "window";

// ---- command results (provider/mod.rs) ----

/** One row of the per-role tier list. */
export interface TierEntry {
  championId: number;
  /** 0..1 */
  winRate: number;
  /** Percentage points vs the previous patch (0.0 = unknown). */
  winRateDelta: number;
  /** Estimated games this patch (0 = unknown; UI falls back to pick rate). */
  games: number;
  /** 0..1 */
  pickRate: number;
  /** 0..1 */
  banRate: number;
}

/** A champion that counters the queried champion; winRate is the counter's. */
export interface CounterEntry {
  championId: number;
  /** 0..1 */
  winRate: number;
  games: number;
}

/** Full rune-page recommendation incl. shards + spells. */
export interface RuneBuild {
  pageName: string;
  /** DeepLoL lane the data came from ("Jungle", …). */
  lane: string;
  /** 0..1 */
  winRate: number;
  games: number;
  primaryStyleId: number;
  subStyleId: number;
  /** [keystone, p1, p2, p3] */
  primaryPerkIds: number[];
  /** [s1, s2] */
  subPerkIds: number[];
  /** [offense, flex, defense] */
  shardIds: number[];
  /** [spell1, spell2]; empty = unknown. */
  spellIds: number[];
  /** True when built against a specific enemy (matchup tab). */
  matchup: boolean;
}

// ---- settings (engine.rs::Settings) ----

export interface Settings {
  autoImportRunes: boolean;
  importSpells: boolean;
  spellsFlipped: boolean;
  /** Champion build provider; migrated from the historical dataSource key. */
  buildDataSource?: string;
  /** Independent player-stat provider. */
  playerStatsSource?: string;
  presentationMode?: PresentationMode;
  developerMode?: boolean;
  autoOpenChampion?: boolean;
  autoOpenLive?: boolean;
}

export interface AppSnapshot {
  phase: PhaseEvent;
  champSelect: ChampSelectEvent;
  windowMode: WindowMode;
}

export interface BuildDetails {
  items: ItemRecommendation[];
  skillOrder?: SkillOrder | null;
}

export interface MobilePairingState {
  status: "disconnected" | "paired" | "error";
  sessionId: string;
  viewerUrl: string;
  pairingCode: string;
  pairingCodeExpiresAt: number;
  expiresAt: number;
  message: string;
}

export interface PanelPosition {
  left: number;
  top: number;
}

export interface WindowGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface UiLayout {
  ingamePanel?: PanelPosition | null;
  controlOverlayWindow?: WindowGeometry | null;
  controlChampselectWindow?: WindowGeometry | null;
  controlIngameWindow?: WindowGeometry | null;
  ingameCollapsed?: boolean;
}

// ---- player stats provider contracts (crates/types/src/player.rs) ----

export interface PlayerRef {
  platformId: string;
  gameName: string;
  tagLine: string;
}

export interface PlayerIdentity extends PlayerRef {
  puuid?: string | null;
}

export type ProviderExtras =
  | { provider: "deeplol" | "ugg" | "opgg"; data: Record<string, unknown> }
  | { provider: "none" };

export interface ProviderCapabilities {
  builds: boolean;
  playerProfile: boolean;
  matchHistory: boolean;
  championStats: boolean;
  liveGame: boolean;
  directApi: boolean;
  siteRefresh: boolean;
  regions: string[];
}

export interface PlayerProviderDescriptor {
  id: string;
  label: string;
  capabilities: ProviderCapabilities;
}

export interface RankedEntry {
  queue: string;
  tier?: string | null;
  division?: string | null;
  lp?: number | null;
  wins?: number | null;
  losses?: number | null;
}

export interface SeasonRank {
  season: string;
  queue: string;
  tier?: string | null;
  division?: string | null;
  lp?: number | null;
}

export interface RefreshAvailability {
  appRefresh: boolean;
  siteRefresh: boolean;
  cooldownUntil?: number | null;
}

export interface PlayerProfile {
  source: string;
  identity: PlayerIdentity;
  level?: number | null;
  profileIconId?: number | null;
  ranks: RankedEntry[];
  previousSeasons: SeasonRank[];
  ladderRank?: number | null;
  ladderPercentile?: number | null;
  fetchedAt: number;
  refresh: RefreshAvailability;
  extras: ProviderExtras;
}

export interface MatchParticipant {
  puuid?: string | null;
  gameName?: string | null;
  tagLine?: string | null;
  championId: number;
  teamId: number;
  role?: string | null;
  win: boolean;
  kills: number;
  deaths: number;
  assists: number;
  items: number[];
  extras: ProviderExtras;
}

export interface PlayerMatch {
  matchId: string;
  startedAt: number;
  durationSeconds: number;
  queueId: number;
  remake: boolean;
  championId: number;
  role?: string | null;
  win: boolean;
  kills: number;
  deaths: number;
  assists: number;
  cs?: number | null;
  items: number[];
  spellIds: number[];
  perkIds: number[];
  participants: MatchParticipant[];
  extras: ProviderExtras;
}

export interface MatchFailure {
  matchId: string;
  message: string;
  retryable: boolean;
}

export interface MatchPage {
  source: string;
  matches: PlayerMatch[];
  nextCursor?: string | null;
  partialFailures: MatchFailure[];
  fetchedAt: number;
}

export interface PlayerChampionStats {
  source: string;
  championId: number;
  games: number;
  wins: number;
  losses: number;
  winRate: number;
  kda: number;
  csPerMinute?: number | null;
  role?: string | null;
  queue: string;
  extras: ProviderExtras;
}

export interface RefreshResult {
  source: string;
  cacheInvalidated: boolean;
  mutationPerformed: boolean;
  refreshedAt: number;
}
