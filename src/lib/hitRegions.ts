import { invoke } from "@tauri-apps/api/core";

let lastHitRegions = "";
let frame = 0;

export function reportHitRegions() {
  const regions = Array.from(document.querySelectorAll<HTMLElement>("[data-hit]"))
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

export function scheduleHitRegionReport() {
  if (frame) return;
  frame = window.requestAnimationFrame(() => {
    frame = 0;
    reportHitRegions();
  });
}

export function startHitRegionInterval() {
  scheduleHitRegionReport();

  const resizeObserver = new ResizeObserver(() => scheduleHitRegionReport());
  const mutationObserver = new MutationObserver(() => {
    syncObservedHitRegions();
    scheduleHitRegionReport();
  });

  const observed = new Set<Element>();
  const syncObservedHitRegions = () => {
    for (const el of document.querySelectorAll("[data-hit]")) {
      if (observed.has(el)) continue;
      observed.add(el);
      resizeObserver.observe(el);
    }
  };

  syncObservedHitRegions();
  mutationObserver.observe(document.body, {
    attributes: true,
    attributeFilter: ["class", "data-hit", "style"],
    childList: true,
    subtree: true,
  });
  window.addEventListener("resize", scheduleHitRegionReport);
  window.addEventListener("scroll", scheduleHitRegionReport, true);
}
