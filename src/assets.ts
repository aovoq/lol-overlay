// Static game data + icon URLs, all from official/community CDNs.
//
// Data Dragon gives us champion / item / summoner-spell names and icons plus
// runesReforged.json (rune trees). Stat shards are NOT in Data Dragon, so they
// live in a hand-verified static table backed by CommunityDragon icons.
// Everything loads once on startup; callers re-render via the returned promise.

const DD = "https://ddragon.leagueoflegends.com";
const CDRAGON_STATMODS =
  "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/perk-images/statmods/";

let ddVersion = ""; // resolved on startup

// ---- lookup tables (populated by initAssets) ----

export interface ChampionInfo {
  /** Image id used in icon filenames, e.g. "MonkeyKing". */
  imageId: string;
  /** Display name, e.g. "Wukong". */
  name: string;
  /** Japanese display name, e.g. "ウーコン" ("" if the ja_JP load failed). */
  nameJa: string;
}
const championByKey = new Map<number, ChampionInfo>();
const championImageByLower = new Map<string, string>();

export interface AbilityInfo {
  name: string;
  icon: string;
}
const abilitiesByChampion = new Map<string, Promise<Map<number, AbilityInfo>>>();

export interface PerkInfo {
  name: string;
  icon: string;
}
const perkById = new Map<number, PerkInfo>();

export interface StyleInfo {
  name: string;
  icon: string;
  /** Tree theme color (design constant — no API exposes these). */
  color: string;
}
const styleById = new Map<number, StyleInfo>();

export interface SpellInfo {
  name: string;
  icon: string;
}
const spellByKey = new Map<number, SpellInfo>();

/** Tree theme colors from the design mocks (see spec §2). */
const STYLE_COLORS: Record<number, string> = {
  8000: "#c8aa6e", // Precision (gold)
  8100: "#e84057", // Domination (red)
  8200: "#9056b8", // Sorcery (purple)
  8300: "#49aab9", // Inspiration (teal)
  8400: "#a1d586", // Resolve (green)
};

// ---- stat shards (static; verified against the live client's perks.json) ----

export interface ShardInfo {
  label: string;
  icon: string;
}

/**
 * Shard id → label + CommunityDragon icon. The 5011/5001 filenames look
 * swapped but are correct (leftover from the 14.2 rework) — do not "fix".
 * Legacy 5002/5003 may still appear in old pages; they render label-only.
 */
const SHARDS: Record<number, ShardInfo> = {
  5008: { label: "+9 Adaptive Force", icon: `${CDRAGON_STATMODS}statmodsadaptiveforceicon.png` },
  5005: { label: "+10% Attack Speed", icon: `${CDRAGON_STATMODS}statmodsattackspeedicon.png` },
  5007: { label: "+8 Ability Haste", icon: `${CDRAGON_STATMODS}statmodscdrscalingicon.png` },
  5010: { label: "+2.5% Move Speed", icon: `${CDRAGON_STATMODS}statmodsmovementspeedicon.png` },
  5001: { label: "+10-180 Health", icon: `${CDRAGON_STATMODS}statmodshealthplusicon.png` },
  5011: { label: "+65 Health", icon: `${CDRAGON_STATMODS}statmodshealthscalingicon.png` },
  5013: {
    label: "+15% Tenacity and Slow Resist",
    icon: `${CDRAGON_STATMODS}statmodstenacityicon.png`,
  },
  5002: { label: "Armor", icon: "" },
  5003: { label: "Magic Resist", icon: "" },
};

// ---- accessors ----

export const profileIconUrl = (id: number) => {
  assetsReady();
  return ddVersion ? `${DD}/cdn/${ddVersion}/img/profileicon/${id}.png` : "";
};

export const itemIconUrl = (id: number) => {
  assetsReady();
  return ddVersion ? `${DD}/cdn/${ddVersion}/img/item/${id}.png` : "";
};

/** Icon by Data Dragon image id (what the Live Client API calls rawName). */
export const champIconByName = (imageId: string) => {
  assetsReady();
  return ddVersion && imageId ? `${DD}/cdn/${ddVersion}/img/champion/${imageId}.png` : "";
};

export const champIconByKey = (key: number) => {
  assetsReady();
  return champIconByName(championByKey.get(key)?.imageId ?? "");
};

/** All champions sorted by display name ([] while assets are loading). */
export const allChampions = (): ({ key: number } & ChampionInfo)[] => {
  assetsReady();
  return [...championByKey.entries()]
    .map(([key, info]) => ({ key, ...info }))
    .sort((a, b) => a.name.localeCompare(b.name));
};

/** Display name for a numeric champion id ("" while assets are loading). */
export const champName = (key: number) => {
  assetsReady();
  return championByKey.get(key)?.name ?? "";
};

/** Data Dragon numeric key for a live-client English champion name (0 = unknown). */
export function championKeyByImage(rawName: string): number {
  const needle = rawName.toLowerCase();
  return allChampions().find((c) => c.imageId.toLowerCase() === needle)?.key ?? 0;
}

export const getPerk = (id: number) => {
  assetsReady();
  return perkById.get(id);
};
export const getStyle = (id: number) => {
  assetsReady();
  return styleById.get(id);
};
export const getSpell = (key: number) => {
  assetsReady();
  return spellByKey.get(key);
};
export const getShard = (id: number): ShardInfo => SHARDS[id] ?? { label: `Shard ${id}`, icon: "" };

export async function getAbility(
  imageId: string,
  skillId: number,
): Promise<AbilityInfo | undefined> {
  if (!ddVersion || !imageId || skillId < 1 || skillId > 4) return undefined;
  const resolvedImageId = championImageByLower.get(imageId.toLowerCase()) ?? imageId;
  let abilities = abilitiesByChampion.get(resolvedImageId);
  if (!abilities) {
    abilities = loadAbilities(resolvedImageId);
    abilitiesByChampion.set(resolvedImageId, abilities);
  }
  return (await abilities).get(skillId);
}

// ---- formatting helpers ----

const trimZero = (x: number) => x.toFixed(1).replace(/\.0$/, "");

/** 563 → "563", 1659 → "1.7k", 3300 → "3.3k". */
export function fmtCompact(n: number): string {
  if (n >= 1_000_000) return `${trimZero(n / 1_000_000)}m`;
  if (n >= 1000) return `${trimZero(n / 1000)}k`;
  return String(Math.round(n));
}

/** 1659 → "1,659". */
export function fmtThousands(n: number): string {
  return Math.round(n)
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Fraction → percent with one decimal: 0.042 → "4.2%". */
export function fmtPct(frac: number): string {
  return `${(frac * 100).toFixed(1)}%`;
}

/** Attach an icon URL, hiding the <img> if there is no URL or the asset 404s. */
export function setIcon(img: HTMLImageElement, url: string) {
  if (!url) {
    img.style.display = "none";
    return;
  }
  img.style.display = "";
  img.onerror = () => {
    img.style.visibility = "hidden";
  };
  img.src = url;
}

// ---- startup loading ----

import { createSignal } from "solid-js";

const [assetsReady, setAssetsReady] = createSignal(false);

export { assetsReady };

let ready: Promise<void> | null = null;

/**
 * Resolve the current Data Dragon version and load champion / rune / spell
 * lookup tables. Idempotent — every caller shares one in-flight promise and
 * can chain a re-render onto it. Failure is non-fatal (icons just stay blank).
 */
export function initAssets(): Promise<void> {
  if (!ready) ready = load();
  return ready;
}

async function load(): Promise<void> {
  try {
    const versions: string[] = await fetch(`${DD}/api/versions.json`).then((r) => r.json());
    ddVersion = versions[0];

    const [champ, runes, spells, champJa] = await Promise.all([
      fetch(`${DD}/cdn/${ddVersion}/data/en_US/champion.json`).then((r) => r.json()),
      fetch(`${DD}/cdn/${ddVersion}/data/en_US/runesReforged.json`).then((r) => r.json()),
      fetch(`${DD}/cdn/${ddVersion}/data/en_US/summoner.json`).then((r) => r.json()),
      // Japanese names for search; non-fatal if unavailable.
      fetch(`${DD}/cdn/${ddVersion}/data/ja_JP/champion.json`)
        .then((r) => r.json())
        .catch(() => null),
    ]);

    const jaNameById = new Map<string, string>();
    for (const c of Object.values<any>(champJa?.data ?? {})) {
      jaNameById.set(c.id, c.name);
    }

    for (const c of Object.values<any>(champ.data)) {
      championByKey.set(Number(c.key), {
        imageId: c.id,
        name: c.name,
        nameJa: jaNameById.get(c.id) ?? "",
      });
      championImageByLower.set(c.id.toLowerCase(), c.id);
    }

    // Rune/style icons use the UNVERSIONED /cdn/img/ root (versioned URLs 403),
    // and paths are not guessable from names — always read `icon` from the JSON.
    for (const style of runes as any[]) {
      styleById.set(style.id, {
        name: style.name,
        icon: `${DD}/cdn/img/${style.icon}`,
        color: STYLE_COLORS[style.id] ?? "#c8aa6e",
      });
      for (const slot of style.slots) {
        for (const rune of slot.runes) {
          perkById.set(rune.id, { name: rune.name, icon: `${DD}/cdn/img/${rune.icon}` });
        }
      }
    }

    // summoner.json keys by internal name; `key` is the numeric id as a string.
    for (const s of Object.values<any>(spells.data)) {
      spellByKey.set(Number(s.key), {
        name: s.name,
        icon: `${DD}/cdn/${ddVersion}/img/spell/${s.image.full}`,
      });
    }
  } catch (e) {
    console.warn("Data Dragon init failed; running without icons", e);
  } finally {
    setAssetsReady(true);
  }
}

async function loadAbilities(imageId: string): Promise<Map<number, AbilityInfo>> {
  try {
    const detail = await fetch(`${DD}/cdn/${ddVersion}/data/en_US/champion/${imageId}.json`).then(
      (r) => r.json(),
    );
    const champ = detail.data?.[imageId] ?? Object.values<any>(detail.data ?? {})[0];
    const abilities = new Map<number, AbilityInfo>();
    for (const [index, spell] of (champ?.spells ?? []).entries()) {
      const full = spell.image?.full ?? "";
      if (!full) continue;
      abilities.set(index + 1, {
        name: spell.name ?? "",
        icon: `${DD}/cdn/${ddVersion}/img/spell/${full}`,
      });
    }
    return abilities;
  } catch (e) {
    console.warn(`Data Dragon ability load failed for ${imageId}`, e);
    return new Map();
  }
}
