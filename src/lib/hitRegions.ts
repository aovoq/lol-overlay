import { invoke } from "@tauri-apps/api/core";

let lastHitRegions = "";

export function reportHitRegions() {
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

export function startHitRegionInterval() {
  reportHitRegions();
  window.setInterval(reportHitRegions, 250);
}
