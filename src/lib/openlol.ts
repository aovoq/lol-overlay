import type { PhaseEvent } from "../types";

export const APP_NAME = "OPENLOL";

const DATA_SOURCE_LABELS: Record<string, string> = {
  deeplol: "DeepLoL",
  ugg: "u.gg",
  lolalytics: "LoLalytics",
  lolps: "LOL.PS",
  opgg: "OP.GG",
};

export const dataSourceLabel = (source: string) => DATA_SOURCE_LABELS[source] ?? source;

export const ROLES = [
  { lcu: "top", chip: "TOP", label: "TOP" },
  { lcu: "jungle", chip: "JG", label: "JUNGLE" },
  { lcu: "middle", chip: "MID", label: "MID" },
  { lcu: "bottom", chip: "BOT", label: "BOT" },
  { lcu: "utility", chip: "SUP", label: "SUPPORT" },
] as const;

export const roleLabel = (lcu: string) =>
  ROLES.find((r) => r.lcu === lcu)?.label ?? lcu.toUpperCase();

export const OPENLOL_MARK_SVG =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2"><polygon points="12 2.5 20.2 7.25 20.2 16.75 12 21.5 3.8 16.75 3.8 7.25"/></svg>';

export function phaseChipLabel(p: PhaseEvent): string {
  if (!p.clientUp) return "OFFLINE";
  const label = p.phase.replace(/([a-z0-9])([A-Z])/g, "$1 $2").toUpperCase();
  return label || "CHAMP SELECT";
}

export function fmtTier(tier: string): string {
  return tier.charAt(0) + tier.slice(1).toLowerCase();
}
