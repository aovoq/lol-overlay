import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type Accessor, createSignal } from "solid-js";
import type { BuildDetails, CounterEntry, RuneBuild, TierEntry } from "../types";

type CacheEntry<T> =
  | { state: "loading" }
  | { state: "ok"; value: T }
  | { state: "err"; error: string };

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

export function makeCache<T>(fetcher: (key: string) => Promise<T>) {
  const map = new Map<
    string,
    {
      entry: Accessor<CacheEntry<T>>;
      set: (e: CacheEntry<T>) => void;
    }
  >();
  const [version, setVersion] = createSignal(0);

  const fire = (key: string, set: (e: CacheEntry<T>) => void) => {
    fetcher(key).then(
      (value) => set({ state: "ok", value }),
      (err) => {
        const message = errorMessage(err);
        console.warn("OPENLOL data fetch failed", { key, error: err, message });
        set({ state: "err", error: message });
      },
    );
  };

  return {
    get(key: string): CacheEntry<T> {
      version();
      let slot = map.get(key);
      if (!slot) {
        const [entry, setEntry] = createSignal<CacheEntry<T>>({
          state: "loading",
        });
        slot = { entry, set: setEntry };
        map.set(key, slot);
        fire(key, setEntry);
      }
      return slot.entry();
    },
    refetch(key: string) {
      const slot = map.get(key);
      if (!slot) return;
      slot.set({ state: "loading" });
      fire(key, slot.set);
    },
    clear() {
      map.clear();
      setVersion((v) => v + 1);
    },
  };
}

export const tierCache = makeCache<TierEntry[]>((role) => invoke("get_tier_list", { role }));

export const counterCache = makeCache<CounterEntry[]>((key) => {
  if (
    import.meta.env.DEV &&
    new URLSearchParams(window.location.search).has("desktop-test") &&
    new URLSearchParams(window.location.search).has("counter-mock")
  ) {
    return Promise.resolve(
      [103, 238, 64, 81, 157, 35, 266, 22].map((championId, index) => ({
        championId,
        winRate: 0.614 - index * 0.019,
        games: 1_200 - index * 80,
      })),
    );
  }
  const [champ, role] = key.split("|");
  return invoke("get_counters", { championId: Number(champ), role });
});

export const buildCache = makeCache<RuneBuild>((key) => {
  const [champ, role, enemy] = key.split("|");
  return invoke("get_rune_build", {
    championId: Number(champ),
    role,
    enemyChampionId: Number(enemy) || null,
  });
});

export const buildDetailsCache = makeCache<BuildDetails>((key) => {
  const [champ, role, enemy] = key.split("|");
  return invoke("get_build_details", {
    championId: Number(champ),
    role,
    enemyChampionId: Number(enemy) || null,
  });
});

export const buildKey = (champ: number, role: string, enemy: number | null) =>
  `${champ}|${role}|${enemy ?? 0}`;

listen<string>("data-source", () => {
  tierCache.clear();
  counterCache.clear();
  buildCache.clear();
  buildDetailsCache.clear();
}).catch(() => {});
