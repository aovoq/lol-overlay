// HEXGATE — the champ-select panel. Owns all champ-select UI state, renders
// into the static markup in index.html, and talks to the backend through the
// champ-select commands (get_tier_list / get_rune_build / get_counters /
// import_build / set_*). The in-game overlay panels live in main.ts.

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type {
  ChampSelectEvent,
  CounterEntry,
  PhaseEvent,
  RuneBuild,
  Settings,
  TierEntry,
  WindowMode,
} from "./types";
import {
  champIconByKey,
  champName,
  fmtCompact,
  fmtPct,
  fmtThousands,
  getPerk,
  getShard,
  getSpell,
  getStyle,
  initAssets,
  setIcon,
} from "./assets";

// ---- constants ----

const ROLES = [
  { lcu: "top", chip: "TOP", label: "TOP" },
  { lcu: "jungle", chip: "JG", label: "JUNGLE" },
  { lcu: "middle", chip: "MID", label: "MID" },
  { lcu: "bottom", chip: "BOT", label: "BOT" },
  { lcu: "utility", chip: "SUP", label: "SUPPORT" },
] as const;

const roleLabel = (lcu: string) =>
  ROLES.find((r) => r.lcu === lcu)?.label ?? lcu.toUpperCase();

const HEX_SVG = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2"><polygon points="12 2.5 20.2 7.25 20.2 16.75 12 21.5 3.8 16.75 3.8 7.25"/></svg>`;

// ---- async invoke cache ----
//
// Every backend lookup is cached per key so hovering a row twice never refires
// a command (the provider caches too, but this keeps hovers instant and the
// render loop synchronous: render functions just read the current entry).

type CacheEntry<T> =
  | { state: "loading" }
  | { state: "ok"; value: T }
  | { state: "err"; error: string };

function makeCache<T>(fetcher: (key: string) => Promise<T>) {
  const map = new Map<string, CacheEntry<T>>();
  return {
    /** Current entry; kicks off the fetch (and a re-render on settle) once. */
    get(key: string): CacheEntry<T> {
      let e = map.get(key);
      if (!e) {
        e = { state: "loading" };
        map.set(key, e);
        fetcher(key).then(
          (value) => {
            map.set(key, { state: "ok", value });
            scheduleRender();
          },
          (err) => {
            const message = errorMessage(err);
            console.warn("HEXGATE data fetch failed", { key, error: err, message });
            map.set(key, { state: "err", error: message });
            scheduleRender();
          },
        );
      }
      return e;
    },
    invalidate(key: string) {
      map.delete(key);
    },
  };
}

function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

const tierCache = makeCache<TierEntry[]>((role) => invoke("get_tier_list", { role }));

const counterCache = makeCache<CounterEntry[]>((key) => {
  const [champ, role] = key.split("|");
  return invoke("get_counters", { championId: Number(champ), role });
});

const buildCache = makeCache<RuneBuild>((key) => {
  const [champ, role, enemy] = key.split("|");
  return invoke("get_rune_build", {
    championId: Number(champ),
    role,
    enemyChampionId: Number(enemy) || null,
  });
});

const buildKey = (champ: number, role: string, enemy: number | null) =>
  `${champ}|${role}|${enemy ?? 0}`;

// ---- state ----

let cs: ChampSelectEvent | null = null;
let windowMode: WindowMode = "overlay";
let pinned = false;
let importSpells = true;
let spellsFlipped = false;
/** Role chosen via the chips when the LCU gives no assigned position. */
let selectedRole = "middle";
let activeTab: "best" | "vs" = "best";
/** Enemy targeted by the VS tab (0 = none revealed yet). */
let vsEnemyId = 0;
/** True once the user retargeted via the ▾ dropdown — stop auto-following. */
let userPickedVsEnemy = false;
/** Champion previewed by hovering a list row / counter icon (0 = none). */
let hoverChampId = 0;
let importState: "idle" | "importing" | "imported" | "failed" = "idle";
let importTimer: number | undefined;
/** Bumped when Data Dragon finishes loading, to force icon/name re-renders. */
let assetsVersion = 0;

// Render-key guards: sections rebuild only when their inputs change, so hover
// listeners and <img> nodes survive unrelated re-renders.
let listsKey = "";
let enemyRowKey = "";
let countersKey = "";
let tabsKey = "";
let buildAreaKey = "";

// ---- DOM handles (markup lives in index.html) ----

const $ = (id: string) => document.getElementById(id)!;
const panel = $("hexgate");
const phaseEl = $("hx-phase");
const pinEl = $("hx-pin") as HTMLButtonElement;
const roleChipsEl = $("hx-role-chips");
const roleLabelEl = $("hx-role-label");
const strongEl = $("hx-strong");
const bansEl = $("hx-bans");
const enemyRowEl = $("hx-enemy-row");
const countersEl = $("hx-counters");
const countersLabelEl = $("hx-counters-label");
const countersStripEl = $("hx-counters-strip");
const matchupEl = $("hx-matchup");
const tabBestEl = $("hx-tab-best") as HTMLButtonElement;
const tabVsEl = $("hx-tab-vs") as HTMLButtonElement;
const vsMenuEl = $("hx-vs-menu");
const buildEl = $("hx-build");
const statsEl = $("hx-stats");
const statWrEl = $("hx-stat-wr");
const statGamesEl = $("hx-stat-games");
const spellIconsEl = $("hx-spell-icons");
const flipEl = $("hx-flip") as HTMLButtonElement;
const spellsOnEl = $("hx-spells-on") as HTMLInputElement;
const importEl = $("hx-import") as HTMLButtonElement;
const proRowEl = $("hx-pro-row");
const settingsSpellsEl = document.getElementById("spells-import") as HTMLInputElement | null;

// ---- derived state ----

const effectiveRole = () => cs?.myRole || selectedRole;
const firstEnemy = () => cs?.enemyChampionIds.find((id) => id > 0) ?? 0;
const revealedEnemies = () => (cs?.enemyChampionIds ?? []).filter((id) => id > 0);

/** What the rune panel currently shows: hover preview > my pick > nothing. */
function displayedTarget(): { champ: number; enemy: number | null } | null {
  if (hoverChampId) return { champ: hoverChampId, enemy: null };
  const my = cs?.myChampionId ?? 0;
  if (!my) return null;
  return { champ: my, enemy: activeTab === "vs" && vsEnemyId ? vsEnemyId : null };
}

// ---- rendering ----

let renderQueued = false;
/** Coalesce renders triggered by event bursts / cache settles. */
function scheduleRender() {
  if (renderQueued) return;
  renderQueued = true;
  queueMicrotask(() => {
    renderQueued = false;
    renderAll();
  });
}

function renderAll() {
  if (!renderVisibility()) return;
  renderRoleChips();
  renderLists(); // before renderBuildArea: a list rebuild clears the hover
  renderEnemyRow();
  renderCounters();
  renderMatchup();
  renderTabs();
  renderBuildArea();
  renderImportButton();
}

/** Panel shows only in ChampSelectMode while champ select is active or pinned. */
function renderVisibility(): boolean {
  const show = windowMode === "champselect" && ((cs?.active ?? false) || pinned);
  panel.classList.toggle("hidden", !show);
  return show;
}

function renderRoleChips() {
  const known = !!cs?.myRole;
  roleChipsEl.classList.toggle("hidden", known || !cs?.active);
  for (const chip of Array.from(roleChipsEl.children)) {
    const el = chip as HTMLElement;
    el.classList.toggle("active", el.dataset.role === selectedRole);
  }
}

function renderLists() {
  const role = effectiveRole();
  const entry = tierCache.get(role);
  const banned = new Set(
    [...(cs?.myBans ?? []), ...(cs?.enemyBans ?? [])].filter((id) => id > 0),
  );
  const key = [role, entry.state, [...banned].sort((a, b) => a - b).join(","), assetsVersion].join("|");
  if (key === listsKey) return;
  listsKey = key;
  hoverChampId = 0; // rebuilding rows orphans any in-flight hover

  roleLabelEl.textContent = roleLabel(role);

  if (entry.state === "loading") {
    strongEl.replaceChildren(...skeletonRows(8));
    bansEl.replaceChildren(...skeletonRows(4));
    return;
  }
  if (entry.state === "err") {
    const retry = () => {
      tierCache.invalidate(role);
      listsKey = "";
      scheduleRender();
    };
    strongEl.replaceChildren(sectionError(retry, entry.error));
    bansEl.replaceChildren(sectionError(retry, entry.error));
    return;
  }

  const tiers = entry.value;
  const strong = tiers
    .filter((t) => t.pickRate >= 0.005)
    .sort((a, b) => b.winRate - a.winRate);
  strongEl.replaceChildren(...strong.map(strongRow));

  const bans = tiers
    .filter((t) => !banned.has(t.championId))
    .sort((a, b) => (b.winRate - 0.5) * b.pickRate - (a.winRate - 0.5) * a.pickRate)
    .slice(0, 10);
  bansEl.replaceChildren(...bans.map(banRow));
}

/** Shared row skeleton: icon + name + hover-to-preview wiring. */
function champRow(championId: number): HTMLDivElement {
  const row = document.createElement("div");
  row.className = "hx-row";
  const icon = document.createElement("img");
  icon.className = "hx-row-icon";
  setIcon(icon, champIconByKey(championId));
  const name = span("hx-row-name", champName(championId) || `#${championId}`);
  row.append(icon, name);
  row.addEventListener("mouseenter", () => setHover(championId));
  row.addEventListener("mouseleave", () => setHover(0));
  return row;
}

function strongRow(t: TierEntry): HTMLDivElement {
  const row = champRow(t.championId);
  const wr = span("hx-row-wr", fmtPct(t.winRate));
  // Delta arrow only when meaningful (|Δ| ≥ 0.5pp); the empty span keeps columns aligned.
  const delta = span("hx-row-delta", "");
  if (Math.abs(t.winRateDelta) >= 0.5) {
    const up = t.winRateDelta > 0;
    delta.classList.add(up ? "up" : "down");
    delta.textContent = `${up ? "▲" : "▼"}${Math.abs(t.winRateDelta).toFixed(1)}`;
  }
  // Games are backend-estimated; 0 means unknown → show pick rate instead.
  const games = span("hx-row-games", t.games > 0 ? fmtCompact(t.games) : fmtPct(t.pickRate));
  row.append(wr, delta, games);
  return row;
}

function banRow(t: TierEntry): HTMLDivElement {
  const row = champRow(t.championId);
  row.append(span("hx-row-wr red", fmtPct(t.winRate)), span("hx-row-games", fmtPct(t.pickRate)));
  return row;
}

function setHover(championId: number) {
  if (hoverChampId === championId) return;
  hoverChampId = championId;
  renderBuildArea();
}

function renderEnemyRow() {
  const ids = cs?.enemyChampionIds.length ? cs.enemyChampionIds : [0, 0, 0, 0, 0];
  const key = ids.join(",") + "|" + assetsVersion;
  if (key === enemyRowKey) return;
  enemyRowKey = key;
  enemyRowEl.replaceChildren(
    ...ids.map((id) => {
      const slot = document.createElement("div");
      slot.className = "hx-enemy-slot";
      if (id > 0) {
        slot.classList.add("revealed");
        slot.title = champName(id);
        const img = document.createElement("img");
        setIcon(img, champIconByKey(id));
        slot.append(img);
      } else {
        slot.textContent = "?";
      }
      return slot;
    }),
  );
}

function renderCounters() {
  const role = effectiveRole();
  const enemy = firstEnemy();
  // Hidden entirely until an enemy pick is revealed.
  countersEl.classList.toggle("hidden", !enemy);
  if (!enemy) {
    countersKey = "";
    return;
  }
  const entry = counterCache.get(`${enemy}|${role}`);
  const key = [enemy, role, entry.state, assetsVersion].join("|");
  if (key === countersKey) return;
  countersKey = key;

  countersLabelEl.textContent = `Counters for ${champName(enemy) || `#${enemy}`}`;
  if (entry.state === "loading") {
    countersStripEl.replaceChildren(
      ...Array.from({ length: 8 }, () => {
        const d = document.createElement("div");
        d.className = "hx-counter hx-skel";
        return d;
      }),
    );
    return;
  }
  if (entry.state === "err" || entry.value.length === 0) {
    countersStripEl.replaceChildren(span("hx-muted", "Not enough data yet"));
    return;
  }
  countersStripEl.replaceChildren(
    ...entry.value.slice(0, 8).map((c) => {
      const item = document.createElement("div");
      item.className = "hx-counter";
      const img = document.createElement("img");
      img.title = champName(c.championId);
      setIcon(img, champIconByKey(c.championId));
      item.append(img, span(`hx-counter-wr${c.winRate > 0.51 ? " up" : ""}`, fmtPct(c.winRate)));
      item.addEventListener("mouseenter", () => setHover(c.championId));
      item.addEventListener("mouseleave", () => setHover(0));
      return item;
    }),
  );
}

function renderMatchup() {
  const my = cs?.myChampionId ?? 0;
  matchupEl.classList.toggle("hidden", !my);
  if (!my) return;

  const parts: (HTMLElement | string)[] = [span("hx-matchup-role", `${roleLabel(effectiveRole())} · `)];
  const meIcon = document.createElement("img");
  meIcon.className = "hx-matchup-icon";
  setIcon(meIcon, champIconByKey(my));
  parts.push(meIcon, span("hx-matchup-me", champName(my) || `#${my}`));

  if (vsEnemyId) {
    parts.push(span("hx-matchup-vs", "vs"));
    const enIcon = document.createElement("img");
    enIcon.className = "hx-matchup-icon";
    setIcon(enIcon, champIconByKey(vsEnemyId));
    parts.push(enIcon, span("hx-matchup-enemy", champName(vsEnemyId) || `#${vsEnemyId}`));
  }
  matchupEl.replaceChildren(...parts);
}

function renderTabs() {
  const key = [activeTab, vsEnemyId, assetsVersion].join("|");
  if (key === tabsKey) return;
  tabsKey = key;

  tabBestEl.classList.toggle("active", activeTab === "best");
  tabVsEl.classList.toggle("active", activeTab === "vs");
  tabVsEl.disabled = !vsEnemyId;
  if (!vsEnemyId) {
    tabVsEl.replaceChildren("VS ...");
    closeVsMenu();
    return;
  }
  const icon = document.createElement("img");
  icon.className = "hx-tab-icon";
  setIcon(icon, champIconByKey(vsEnemyId));
  tabVsEl.replaceChildren(
    icon,
    ` VS ${(champName(vsEnemyId) || `#${vsEnemyId}`).toUpperCase()} `,
    span("hx-chevron", "▾"),
  );
}

function openVsMenu() {
  vsMenuEl.replaceChildren(
    ...revealedEnemies().map((id) => {
      const item = document.createElement("button");
      item.className = "hx-vs-item";
      const icon = document.createElement("img");
      setIcon(icon, champIconByKey(id));
      item.append(icon, span("", champName(id) || `#${id}`));
      item.addEventListener("click", () => {
        vsEnemyId = id;
        userPickedVsEnemy = true;
        activeTab = "vs";
        closeVsMenu();
        scheduleRender();
      });
      return item;
    }),
  );
  vsMenuEl.classList.remove("hidden");
}

function closeVsMenu() {
  vsMenuEl.classList.add("hidden");
}

function renderBuildArea() {
  const role = effectiveRole();
  const target = displayedTarget();

  if (!target) {
    const key = `empty|${role}|${assetsVersion}`;
    if (key === buildAreaKey) return;
    buildAreaKey = key;
    buildEl.replaceChildren(bigEmptyState(role));
    statsEl.classList.add("hidden");
    return;
  }

  const cacheKey = buildKey(target.champ, role, target.enemy);
  const entry = buildCache.get(cacheKey);
  const key = [cacheKey, entry.state, assetsVersion, spellsFlipped].join("|");
  if (key === buildAreaKey) return;
  buildAreaKey = key;

  if (entry.state === "loading") {
    buildEl.replaceChildren(buildSkeleton());
    statsEl.classList.add("hidden");
    return;
  }
  if (entry.state === "err") {
    statsEl.classList.add("hidden");
    if (entry.error === "not-enough-data") {
      buildEl.replaceChildren(notEnoughDataState(target.champ, target.enemy !== null));
    } else {
      buildEl.replaceChildren(
        sectionError(() => {
          buildCache.invalidate(cacheKey);
          buildAreaKey = "";
          scheduleRender();
        }, entry.error),
      );
    }
    return;
  }
  buildEl.replaceChildren(runePage(entry.value));
  renderStats(entry.value);
}

function renderStats(b: RuneBuild) {
  statWrEl.textContent = `${fmtPct(b.winRate)} WR`;
  statGamesEl.textContent = ` · ${fmtThousands(b.games)} games`;
  const spells = spellsFlipped ? [...b.spellIds].reverse() : b.spellIds;
  spellIconsEl.replaceChildren(
    ...spells.map((id) => {
      const img = document.createElement("img");
      img.title = getSpell(id)?.name ?? `Spell ${id}`;
      setIcon(img, getSpell(id)?.icon ?? "");
      return img;
    }),
  );
  flipEl.classList.toggle("hidden", b.spellIds.length < 2);
  statsEl.classList.remove("hidden");
}

// ---- rune page DOM ----

function runePage(b: RuneBuild): HTMLElement {
  const root = document.createElement("div");
  root.className = "hx-runes";

  root.append(treeHead(b.primaryStyleId, true));
  const [keystone, ...minors] = b.primaryPerkIds;
  if (keystone !== undefined) root.append(keystoneCard(keystone));
  for (const id of minors) root.append(runeRow(id));

  root.append(treeHead(b.subStyleId, false));
  for (const id of b.subPerkIds) root.append(runeRow(id));

  root.append(span("hx-shards-head", "SHARDS"));
  const shards = document.createElement("div");
  shards.className = "hx-shards";
  for (const id of b.shardIds) shards.append(shardChip(id));
  root.append(shards);
  return root;
}

/** Tree header; primary stays gold, secondary uses the tree's theme color. */
function treeHead(styleId: number, primary: boolean): HTMLElement {
  const s = getStyle(styleId);
  const head = document.createElement("div");
  head.className = "hx-tree-head";
  if (!primary && s) head.style.color = s.color;
  const icon = document.createElement("img");
  setIcon(icon, s?.icon ?? "");
  head.append(icon, span("", (s?.name ?? `Style ${styleId}`).toUpperCase()));
  return head;
}

function keystoneCard(perkId: number): HTMLElement {
  const card = document.createElement("div");
  card.className = "hx-keystone";
  const icon = document.createElement("img");
  setIcon(icon, getPerk(perkId)?.icon ?? "");
  card.append(icon, span("hx-keystone-name", getPerk(perkId)?.name ?? `#${perkId}`));
  return card;
}

function runeRow(perkId: number): HTMLElement {
  const row = document.createElement("div");
  row.className = "hx-rune-row";
  const icon = document.createElement("img");
  setIcon(icon, getPerk(perkId)?.icon ?? "");
  row.append(icon, span("", getPerk(perkId)?.name ?? `#${perkId}`));
  return row;
}

function shardChip(shardId: number): HTMLElement {
  const chip = document.createElement("div");
  chip.className = "hx-shard";
  const info = getShard(shardId);
  if (info.icon) {
    const icon = document.createElement("img");
    setIcon(icon, info.icon);
    chip.append(icon);
  }
  chip.append(span("", info.label));
  return chip;
}

// ---- empty / error / skeleton states ----

function span(cls: string, text: string): HTMLSpanElement {
  const s = document.createElement("span");
  if (cls) s.className = cls;
  s.textContent = text;
  return s;
}

function hexGlyph(): HTMLElement {
  const d = document.createElement("div");
  d.className = "hx-glyph";
  d.innerHTML = HEX_SVG;
  return d;
}

function bigEmptyState(role: string): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "hx-empty";
  const title = document.createElement("div");
  title.className = "hx-empty-role";
  title.textContent = roleLabel(role);
  const hint = document.createElement("div");
  hint.className = "hx-empty-text";
  hint.textContent = "Hover a champion to see runes";
  wrap.append(hexGlyph(), title, hint);
  return wrap;
}

function notEnoughDataState(championId: number, matchup: boolean): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "hx-empty";
  const title = document.createElement("div");
  title.className = "hx-empty-title";
  title.textContent = "Not enough data";
  wrap.append(hexGlyph(), title);
  if (matchup) {
    const text = document.createElement("div");
    text.className = "hx-empty-text";
    text.textContent = `Too few games for ${champName(championId) || "this champion"} in this matchup to provide reliable rune recommendations.`;
    wrap.append(text);
  }
  return wrap;
}

function sectionError(onRetry: () => void, message = ""): HTMLElement {
  const d = document.createElement("div");
  d.className = "hx-error";
  d.append("Couldn't load data ");
  const retry = document.createElement("button");
  retry.className = "hx-retry";
  retry.textContent = "Retry";
  retry.addEventListener("click", onRetry);
  d.append(retry);
  if (message) {
    const detail = document.createElement("div");
    detail.className = "hx-error-detail";
    detail.textContent = message;
    d.append(detail);
  }
  return d;
}

function skeletonRows(n: number): HTMLElement[] {
  return Array.from({ length: n }, () => {
    const d = document.createElement("div");
    d.className = "hx-row hx-skel";
    return d;
  });
}

function buildSkeleton(): HTMLElement {
  const root = document.createElement("div");
  root.className = "hx-runes";
  const keystone = document.createElement("div");
  keystone.className = "hx-skel hx-skel-keystone";
  root.append(keystone);
  for (let i = 0; i < 5; i++) {
    const row = document.createElement("div");
    row.className = "hx-skel hx-skel-row";
    root.append(row);
  }
  return root;
}

// ---- import button ----

function renderImportButton() {
  const my = cs?.myChampionId ?? 0;
  importEl.classList.toggle("hidden", !my);
  proRowEl.classList.toggle("hidden", !my);
  importEl.disabled = importState === "importing";
  importEl.classList.toggle("failed", importState === "failed");
  switch (importState) {
    case "importing":
      importEl.textContent = "IMPORTING…";
      break;
    case "imported":
      importEl.textContent = "IMPORTED ✓";
      break;
    case "failed":
      importEl.textContent = "FAILED — RETRY";
      break;
    default:
      importEl.textContent = importSpells ? "IMPORT RUNES & SPELLS" : "IMPORT RUNES";
  }
}

function finishImport(state: "imported" | "failed", revertAfterMs: number) {
  importState = state;
  renderImportButton();
  importTimer = window.setTimeout(() => {
    importState = "idle";
    renderImportButton();
  }, revertAfterMs);
}

// ---- settings sync ----

/** Push a user toggle to the backend and mirror it across both checkboxes. */
function setImportSpells(on: boolean) {
  importSpells = on;
  spellsOnEl.checked = on;
  if (settingsSpellsEl) settingsSpellsEl.checked = on;
  invoke("set_import_spells", { enabled: on }).catch(() => {});
  renderImportButton();
}

/** Initialize UI state from the backend settings (no invokes back). */
export function applySettings(s: Partial<Settings>) {
  importSpells = s.importSpells ?? importSpells;
  spellsFlipped = s.spellsFlipped ?? spellsFlipped;
  pinned = s.pinned ?? pinned;
  spellsOnEl.checked = importSpells;
  if (settingsSpellsEl) settingsSpellsEl.checked = importSpells;
  pinEl.classList.toggle("active", pinned);
  buildAreaKey = ""; // spell order may have changed
  scheduleRender();
}

// ---- event handling ----

function onChampSelect(e: ChampSelectEvent) {
  cs = e;
  const revealed = revealedEnemies();
  // VS target: default to the first revealed enemy; honor a manual retarget
  // for as long as that enemy is still on the board.
  if (!revealed.includes(vsEnemyId)) {
    vsEnemyId = revealed[0] ?? 0;
    userPickedVsEnemy = false;
  } else if (!userPickedVsEnemy && revealed.length > 0) {
    vsEnemyId = revealed[0];
  }
  if (!vsEnemyId && activeTab === "vs") activeTab = "best";
  if (!e.active) hoverChampId = 0;
  scheduleRender();
}

/** "ChampSelect" → "CHAMP SELECT" etc. for the header chip. */
function phaseChipLabel(p: PhaseEvent): string {
  if (!p.clientUp) return "OFFLINE";
  const label = p.phase.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toUpperCase();
  return label || "CHAMP SELECT";
}

// ---- init ----

let initialized = false;

export function initChampSelect() {
  if (initialized) return;
  initialized = true;

  // Role chips (blind pick): TOP/JG/MID/BOT/SUP, default MID.
  for (const r of ROLES) {
    const chip = document.createElement("button");
    chip.className = "hx-role-chip";
    chip.dataset.role = r.lcu;
    chip.textContent = r.chip;
    chip.addEventListener("click", () => {
      if (selectedRole === r.lcu) return;
      selectedRole = r.lcu;
      scheduleRender();
    });
    roleChipsEl.append(chip);
  }

  tabBestEl.addEventListener("click", () => {
    activeTab = "best";
    scheduleRender();
  });
  tabVsEl.addEventListener("click", (ev) => {
    if (!vsEnemyId) return;
    if ((ev.target as HTMLElement).closest(".hx-chevron")) {
      if (vsMenuEl.classList.contains("hidden")) openVsMenu();
      else closeVsMenu();
      return;
    }
    activeTab = "vs";
    scheduleRender();
  });
  document.addEventListener("click", (ev) => {
    const t = ev.target as HTMLElement;
    if (!t.closest("#hx-vs-menu") && !t.closest("#hx-tab-vs")) closeVsMenu();
  });

  pinEl.addEventListener("click", () => {
    pinned = !pinned;
    invoke("set_pinned", { pinned }).catch(() => {});
    pinEl.classList.toggle("active", pinned);
    renderVisibility();
  });

  flipEl.addEventListener("click", () => {
    spellsFlipped = !spellsFlipped; // optimistic — backend write is fire-and-forget
    invoke("set_spells_flipped", { flipped: spellsFlipped }).catch(() => {});
    buildAreaKey = "";
    renderBuildArea();
  });

  spellsOnEl.addEventListener("change", () => setImportSpells(spellsOnEl.checked));
  if (settingsSpellsEl) {
    const el = settingsSpellsEl;
    el.addEventListener("change", () => setImportSpells(el.checked));
  }

  importEl.addEventListener("click", () => {
    const my = cs?.myChampionId ?? 0;
    if (!my || importState === "importing") return;
    if (importTimer) window.clearTimeout(importTimer);
    importState = "importing";
    renderImportButton();
    invoke("import_build", {
      championId: my,
      role: effectiveRole(),
      enemyChampionId: activeTab === "vs" && vsEnemyId ? vsEnemyId : null,
      includeSpells: importSpells,
      flipSpells: spellsFlipped,
    }).then(
      () => finishImport("imported", 2000),
      () => finishImport("failed", 3000),
    );
  });

  listen<ChampSelectEvent>("champ-select", (e) => onChampSelect(e.payload));

  listen<WindowMode>("window-mode", (e) => {
    windowMode = e.payload;
    document.body.classList.toggle("champselect", windowMode === "champselect");
    scheduleRender();
  });

  listen<PhaseEvent>("phase", (e) => {
    phaseEl.textContent = phaseChipLabel(e.payload);
  });

  initAssets().then(() => {
    assetsVersion++;
    scheduleRender();
  });

  scheduleRender();
}
