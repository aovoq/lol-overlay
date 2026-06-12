// Entry point: wires the in-game overlay panels (status chip, item
// recommendations, rune-import banner, settings) and the HEXGATE champ-select
// panel (champselect.ts). Payload types live in types.ts, CDN assets in
// assets.ts.

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  LogEvent,
  LpChangeEvent,
  PhaseEvent,
  RecentGame,
  RecommendationsEvent,
  RuneImportedEvent,
  Settings,
  SummonerEvent,
  SkillOrder,
  UiLayout,
  WindowMode,
} from "./types";
import {
  champIconByKey,
  champIconByName,
  champName,
  getAbility,
  initAssets,
  itemIconUrl,
  profileIconUrl,
  setIcon,
} from "./assets";
import { applySettings, initChampSelect } from "./champselect";

// ---- DOM handles ----

const $ = (id: string) => document.getElementById(id)!;
const dot = $("dot");
const statusText = $("status-text");
const recs = $("recs");
const recList = $("rec-list");
const recsTitle = $("recs-title");
const recsChamp = $("recs-champ") as HTMLImageElement;
const recsPos = $("recs-pos");
const threatsEl = $("threats");
const enemiesEl = $("enemies");
const skillOrderEl = $("skill-order");
const skillPath = $("skill-path");
const profileEl = $("profile");
const profileForm = $("profile-form");
const formGames = $("form-games");
const formSummary = $("form-summary");
const lpBanner = $("lp-banner");
const lpResult = $("lp-result");
const lpDetail = $("lp-detail");
const profileIcon = $("profile-icon") as HTMLImageElement;
const profileName = $("profile-name");
const profileRank = $("profile-rank");
const runeBanner = $("rune-banner");
const runeName = $("rune-name");
const runeChamp = $("rune-champ") as HTMLImageElement;

let runeBannerTimer: number | undefined;
let wasInGame = false;
let lastRecs: RecommendationsEvent | null = null;
let lastSummoner: SummonerEvent | null = null;
let lastHistory: RecentGame[] | null = null;
let lpBannerTimer: number | undefined;
let currentWindowMode: WindowMode = "overlay";

// ---- hit regions (region-based click-through) ----
//
// The overlay window is click-through except while the cursor sits inside the
// rect of a visible [data-hit] element: the backend polls the cursor against
// these rects and flips click-through on transitions (src-tauri/src/hittest.rs).
// A cheap interval keeps the rects fresh across renders, panel moves and
// window-mode switches; hot paths (drag end, collapse) also report directly.

let lastHitRegions = "";

function reportHitRegions() {
  const regions = Array.from(
    document.querySelectorAll<HTMLElement>("[data-hit]"),
  )
    .map((el) => el.getBoundingClientRect())
    .filter((r) => r.width > 0 && r.height > 0)
    .map((r) => ({
      left: Math.floor(r.left),
      top: Math.floor(r.top),
      width: Math.ceil(r.width),
      height: Math.ceil(r.height),
    }));
  const key = JSON.stringify(regions);
  if (key === lastHitRegions) return;
  lastHitRegions = key;
  invoke("set_hit_regions", { regions }).catch(() => {});
}

// ---- window dragging ----

const appWindow = getCurrentWindow();
let champselectMoveSaveTimer: number | undefined;

function shouldStartDrag(event: PointerEvent): boolean {
  if (event.button !== 0) return false;
  const target = event.target;
  if (!(target instanceof Element)) return true;
  return !target.closest("button, input, label, select, textarea, a");
}

function applyPanelPosition(panel: HTMLElement, left: number, top: number) {
  panel.style.left = `${Math.max(0, left)}px`;
  panel.style.top = `${Math.max(0, top)}px`;
  panel.style.right = "auto";
  panel.style.bottom = "auto";
}

function clampPanelToViewport(panel: HTMLElement) {
  const rect = panel.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return;

  const left = Math.min(
    Math.max(0, rect.left),
    Math.max(0, window.innerWidth - rect.width),
  );
  const top = Math.min(
    Math.max(0, rect.top),
    Math.max(0, window.innerHeight - rect.height),
  );
  applyPanelPosition(panel, left, top);
}

function saveIngamePanelPosition(panel: HTMLElement) {
  const rect = panel.getBoundingClientRect();
  invoke("set_ingame_panel_position", {
    left: Math.round(rect.left),
    top: Math.round(rect.top),
  }).catch(() => {});
}

async function saveChampselectWindowPosition(position: { x: number; y: number }) {
  const scale = await appWindow.scaleFactor();
  await invoke("set_champselect_window_position", {
    x: Math.round(position.x / scale),
    y: Math.round(position.y / scale),
  });
}

function initWindowDragHandles() {
  const header = document.querySelector<HTMLElement>(".hx-header");
  header?.addEventListener("pointerdown", (event) => {
    if (!shouldStartDrag(event)) return;
    event.preventDefault();
    appWindow.startDragging().catch(() => {});
  });

  appWindow
    .onMoved(({ payload }) => {
      if (currentWindowMode !== "champselect") return;
      if (champselectMoveSaveTimer) window.clearTimeout(champselectMoveSaveTimer);
      champselectMoveSaveTimer = window.setTimeout(() => {
        if (currentWindowMode !== "champselect") return;
        saveChampselectWindowPosition(payload).catch(() => {});
      }, 250);
    })
    .catch(() => {});
}

function initPanelDragHandle(panel: HTMLElement, handle: HTMLElement | null) {
  handle?.addEventListener("pointerdown", (event) => {
    if (!shouldStartDrag(event)) return;
    event.preventDefault();

    const rect = panel.getBoundingClientRect();
    const startX = event.clientX;
    const startY = event.clientY;
    const startLeft = rect.left;
    const startTop = rect.top;

    panel.style.left = `${startLeft}px`;
    panel.style.top = `${startTop}px`;
    panel.style.right = "auto";
    panel.style.bottom = "auto";
    handle.setPointerCapture(event.pointerId);
    // Hold the window interactive: a fast drag outruns the reported rects.
    invoke("set_drag_active", { active: true }).catch(() => {});

    const onPointerMove = (moveEvent: PointerEvent) => {
      const maxLeft = Math.max(0, window.innerWidth - rect.width);
      const maxTop = Math.max(0, window.innerHeight - rect.height);
      const left = Math.min(
        Math.max(0, startLeft + moveEvent.clientX - startX),
        maxLeft,
      );
      const top = Math.min(
        Math.max(0, startTop + moveEvent.clientY - startY),
        maxTop,
      );
      panel.style.left = `${left}px`;
      panel.style.top = `${top}px`;
    };

    const stopDragging = () => {
      if (handle.hasPointerCapture(event.pointerId)) {
        handle.releasePointerCapture(event.pointerId);
      }
      handle.removeEventListener("pointermove", onPointerMove);
      handle.removeEventListener("pointerup", stopDragging);
      handle.removeEventListener("pointercancel", stopDragging);
      clampPanelToViewport(panel);
      saveIngamePanelPosition(panel);
      invoke("set_drag_active", { active: false }).catch(() => {});
      reportHitRegions();
    };

    handle.addEventListener("pointermove", onPointerMove);
    handle.addEventListener("pointerup", stopDragging);
    handle.addEventListener("pointercancel", stopDragging);
  });
}

function applyUiLayout(layout: UiLayout) {
  const ingame = layout.ingamePanel;
  if (ingame) {
    applyPanelPosition(recs, ingame.left, ingame.top);
  }
  applyIngameCollapsed(layout.ingameCollapsed ?? false);
}

// ---- in-game panel collapse (header chevron) ----

function applyIngameCollapsed(collapsed: boolean) {
  recs.classList.toggle("collapsed", collapsed);
  reportHitRegions();
}

$("ig-collapse").addEventListener("click", () => {
  const collapsed = !recs.classList.contains("collapsed");
  applyIngameCollapsed(collapsed);
  invoke("set_ingame_collapsed", { collapsed }).catch(() => {});
});

// After the collapse/expand width transition settles: pull the panel back on
// screen if expanding pushed it past an edge, and refresh the header rect.
recs.addEventListener("transitionend", (event) => {
  if (event.target !== recs || event.propertyName !== "width") return;
  clampPanelToViewport(recs);
  saveIngamePanelPosition(recs);
  reportHitRegions();
});

// ---- rendering ----

function renderPhase(p: PhaseEvent) {
  dot.className = "dot";
  if (p.inGame) {
    dot.classList.add("ingame");
    statusText.textContent = `In game (${p.phase})`;
  } else if (p.clientUp) {
    dot.classList.add("up");
    statusText.textContent = `Client: ${p.phase}`;
  } else {
    statusText.textContent = "Waiting for League client…";
  }
  if (wasInGame && !p.inGame) recs.classList.add("hidden");
  wasInGame = p.inGame;
}

/** One colored chip of the enemy damage-profile row ("2 AD" etc.). */
function threatChip(kind: string, count: number, label: string): HTMLElement {
  const chip = document.createElement("span");
  chip.className = `threat-chip ${kind}`;
  const b = document.createElement("b");
  b.textContent = String(count);
  chip.append(b, ` ${label}`);
  return chip;
}

function skillLabel(skillId: number): string {
  return ["", "Q", "W", "E", "R"][skillId] ?? "";
}

function isBasicSkill(skillId: number): boolean {
  return skillId >= 1 && skillId <= 3;
}

function skillOrderIds(order: SkillOrder | null | undefined): number[] {
  const maxOrder = order?.maxOrder.filter(isBasicSkill) ?? [];
  if (maxOrder.length > 0) return maxOrder.slice(0, 3);

  const derived: number[] = [];
  for (const skillId of order?.levelOrder ?? []) {
    if (!isBasicSkill(skillId) || derived.includes(skillId)) continue;
    derived.push(skillId);
    if (derived.length === 3) break;
  }
  return derived;
}

function skillCard(skillId: number, championImageId: string): HTMLElement {
  const label = skillLabel(skillId);
  const card = document.createElement("span");
  card.className = "skill-card";
  card.title = label;

  const img = document.createElement("img");
  img.className = "skill-card-icon";
  img.alt = "";
  img.style.visibility = "hidden";

  const key = document.createElement("span");
  key.className = "skill-card-key";
  key.textContent = label;

  card.append(img, key);

  void getAbility(championImageId, skillId).then((ability) => {
    if (!ability) return;
    img.style.visibility = "";
    img.alt = ability.name;
    setIcon(img, ability.icon);
    card.classList.add("has-icon");
    card.title = ability.name ? `${label} · ${ability.name}` : label;
  });

  return card;
}

function skillArrow(): HTMLElement {
  const arrow = document.createElement("span");
  arrow.className = "skill-arrow";
  return arrow;
}

function renderSkillOrder(
  order: SkillOrder | null | undefined,
  championImageId: string,
) {
  const ids = skillOrderIds(order);

  if (ids.length === 0) {
    skillOrderEl.classList.add("hidden");
    skillPath.replaceChildren();
    return;
  }

  skillOrderEl.title =
    order && order.games > 0
      ? `${Math.round(order.winRate * 100)}% WR · ${order.games} games`
      : "";

  const nodes: HTMLElement[] = [];
  ids.forEach((skillId, index) => {
    nodes.push(skillCard(skillId, championImageId));
    if (index < ids.length - 1) nodes.push(skillArrow());
  });
  skillPath.replaceChildren(...nodes);
  skillOrderEl.classList.remove("hidden");
}

function renderRecommendations(e: RecommendationsEvent) {
  lastRecs = e;

  recsTitle.textContent = e.selfChampion || "—";
  recsPos.textContent = e.selfPosition || "";
  setIcon(recsChamp, champIconByName(e.selfRawName));

  const t = e.threats;
  const chips = [
    threatChip("ad", t.adCount, "AD"),
    threatChip("ap", t.apCount, "AP"),
    threatChip("tank", t.tankCount, "TANK"),
  ];
  if (t.ccHeavy) {
    const cc = document.createElement("span");
    cc.className = "threat-chip cc";
    cc.textContent = "CC HEAVY";
    chips.push(cc);
  }
  threatsEl.replaceChildren(...chips);

  // Enemy champion icons.
  enemiesEl.replaceChildren(
    ...e.enemies.map((en) => {
      const img = document.createElement("img");
      img.className = "champ-icon sm";
      img.title = en.name;
      setIcon(img, champIconByName(en.rawName));
      return img;
    }),
  );

  renderSkillOrder(e.skillOrder, e.selfRawName);

  // Item recommendations with icons.
  recList.replaceChildren(
    ...e.items.map((it) => {
      const li = document.createElement("li");

      const icon = document.createElement("img");
      icon.className = "item-icon";
      setIcon(icon, itemIconUrl(it.itemId));

      const text = document.createElement("div");
      text.className = "rec-text";

      const name = document.createElement("span");
      name.className = "name";
      name.textContent = it.name;

      const reason = document.createElement("span");
      reason.className = "reason";
      reason.textContent = it.reason;

      const bar = document.createElement("div");
      bar.className = "score-bar";
      bar.style.width = `${Math.round(it.score * 100)}%`;

      text.append(name, reason, bar);
      li.append(icon, text);
      return li;
    }),
  );

  recs.classList.remove("hidden");
  clampPanelToViewport(recs);
}

/** Title-case a tier name from the LCU ("EMERALD" → "Emerald"). */
function fmtTier(tier: string): string {
  return tier.charAt(0) + tier.slice(1).toLowerCase();
}

function renderSummoner(e: SummonerEvent | null) {
  lastSummoner = e;
  if (!e) {
    profileEl.classList.add("hidden");
    return;
  }

  profileName.textContent = e.tagLine
    ? `${e.gameName} #${e.tagLine}`
    : e.gameName;
  setIcon(profileIcon, profileIconUrl(e.profileIconId));

  if (e.soloTier) {
    const division = e.soloDivision && e.soloDivision !== "NA"
      ? ` ${e.soloDivision}`
      : "";
    const games = e.soloWins + e.soloLosses;
    const winRate = games > 0
      ? ` · ${e.soloWins}W ${e.soloLosses}L (${Math.round((e.soloWins / games) * 100)}%)`
      : "";
    profileRank.textContent =
      `${fmtTier(e.soloTier)}${division} ${e.soloLp} LP${winRate}`;
  } else {
    profileRank.textContent = "Unranked";
  }
  profileEl.classList.remove("hidden");
}

function renderMatchHistory(games: RecentGame[]) {
  lastHistory = games;
  if (games.length === 0) {
    profileForm.classList.add("hidden");
    return;
  }

  const wins = games.filter((g) => g.win).length;
  const losses = games.length - wins;
  const kills = games.reduce((n, g) => n + g.kills, 0);
  const deaths = games.reduce((n, g) => n + g.deaths, 0);
  const assists = games.reduce((n, g) => n + g.assists, 0);
  const kda = deaths > 0 ? ((kills + assists) / deaths).toFixed(2) : "Perfect";

  // Win/loss streak counted from the newest game.
  let streak = 1;
  while (streak < games.length && games[streak].win === games[0].win) streak++;
  const streakLabel =
    streak >= 2 ? ` · ${streak}${games[0].win ? "連勝" : "連敗"}` : "";

  formGames.replaceChildren(
    ...games.map((g) => {
      const img = document.createElement("img");
      img.className = `form-game ${g.win ? "win" : "loss"}`;
      const name = champName(g.championId);
      img.title = `${name ? `${name} · ` : ""}${g.kills}/${g.deaths}/${g.assists} · ${g.win ? "勝利" : "敗北"}`;
      setIcon(img, champIconByKey(g.championId));
      return img;
    }),
  );
  formSummary.textContent = `${wins}W ${losses}L · KDA ${kda}${streakLabel}`;
  formSummary.classList.toggle("streak-win", games[0].win && streak >= 3);
  formSummary.classList.toggle("streak-loss", !games[0].win && streak >= 3);
  profileForm.classList.remove("hidden");
}

function renderLpChange(e: LpChangeEvent) {
  const division = e.division && e.division !== "NA" ? ` ${e.division}` : "";
  const rankNow = `${fmtTier(e.tier)}${division} · ${e.lp} LP`;

  lpBanner.classList.remove("win", "loss");
  if (e.rankChange === "promoted") {
    lpBanner.classList.add("win");
    lpResult.textContent = `昇格! ${fmtTier(e.tier)}${division}`;
    lpDetail.textContent = `${e.lp} LP スタート`;
  } else if (e.rankChange === "demoted") {
    lpBanner.classList.add("loss");
    lpResult.textContent = `降格 ${fmtTier(e.tier)}${division}`;
    lpDetail.textContent = `${e.lp} LP`;
  } else {
    lpBanner.classList.add(e.win ? "win" : "loss");
    const sign = e.lpDelta >= 0 ? "+" : "";
    lpResult.textContent = `${e.win ? "VICTORY" : "DEFEAT"} ${sign}${e.lpDelta} LP`;
    lpDetail.textContent = rankNow;
  }

  lpBanner.classList.remove("hidden");
  if (lpBannerTimer) window.clearTimeout(lpBannerTimer);
  lpBannerTimer = window.setTimeout(
    () => lpBanner.classList.add("hidden"),
    12000,
  );
}

function renderRuneImport(e: RuneImportedEvent) {
  runeName.textContent = e.pageName;
  setIcon(runeChamp, champIconByKey(e.championId));
  runeBanner.classList.remove("hidden");
  if (runeBannerTimer) window.clearTimeout(runeBannerTimer);
  runeBannerTimer = window.setTimeout(
    () => runeBanner.classList.add("hidden"),
    6000,
  );
}

// ---- wire up backend events ----

listen<PhaseEvent>("phase", (e) => renderPhase(e.payload));
listen<RecommendationsEvent>("recommendations", (e) =>
  renderRecommendations(e.payload),
);
listen<RuneImportedEvent>("rune-imported", (e) => renderRuneImport(e.payload));
listen<SummonerEvent | null>("summoner", (e) => renderSummoner(e.payload));
listen<RecentGame[]>("match-history", (e) => renderMatchHistory(e.payload));
listen<LpChangeEvent>("lp-change", (e) => renderLpChange(e.payload));
listen<LogEvent>("log", (e) =>
  console.log(`[${e.payload.level}] ${e.payload.message}`),
);
listen<WindowMode>("window-mode", (e) => {
  currentWindowMode = e.payload;
});

// ---- settings panel (gear in the champ-select header, or the Ctrl+Shift+O
// emergency override) ----

const settings = $("settings");
const autoImport = $("auto-import") as HTMLInputElement;

$("hx-gear").addEventListener("click", () => {
  settings.classList.toggle("hidden");
});

invoke<Settings>("get_settings")
  .then((s) => {
    autoImport.checked = s.autoImportRunes ?? true;
    applySettings(s);
  })
  .catch(() => {});

invoke<UiLayout>("get_ui_layout")
  .then((layout) => applyUiLayout(layout))
  .catch(() => {});

autoImport.addEventListener("change", () => {
  invoke("set_auto_import", { enabled: autoImport.checked }).catch(() => {});
});

listen<boolean>("interactive", (e) => {
  const on = e.payload;
  settings.classList.toggle("hidden", !on);
  document.body.classList.toggle("interactive", on);
});

// ---- startup ----

initWindowDragHandles();
initPanelDragHandle(recs, document.querySelector<HTMLElement>(".ig-head"));
initChampSelect();

reportHitRegions();
window.setInterval(reportHitRegions, 250);

window.addEventListener("resize", () => {
  if (recs.classList.contains("hidden")) return;
  clampPanelToViewport(recs);
  saveIngamePanelPosition(recs);
});

initAssets().then(() => {
  // Re-render whatever is on screen now that icons are available.
  if (lastRecs) renderRecommendations(lastRecs);
  if (lastSummoner) renderSummoner(lastSummoner);
  if (lastHistory) renderMatchHistory(lastHistory);
});
