import { Input } from "@msviderok/base-ui-solid/input";
import { createMemo, createSignal, For, Show } from "solid-js";
import { allChampions, type ChampionInfo, champIconByName } from "../../assets";

/** Lowercase, strip diacritics/punctuation/spaces, fold katakana (incl.
 * half-width) into hiragana — so "kai sa", "ナーフィリ" and "なーふぃり" all
 * normalize to comparable strings. */
function normalizeForSearch(s: string): string {
  return s
    .normalize("NFKC") // half-width katakana → full-width, etc.
    .toLowerCase()
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "") // diacritics: Bél'Veth → bel'veth
    .replace(/[ァ-ヶ]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0x60)) // カ → か
    .replace(/[\s'.&・=ー-]/g, "");
}

/** True if every char of `query` appears in `target` in order (nafi → naafiri). */
function isSubsequence(query: string, target: string): boolean {
  let i = 0;
  for (const c of target) {
    if (c === query[i]) i++;
    if (i === query.length) return true;
  }
  return i === query.length;
}

/** Match rank: 0 = prefix, 1 = substring, 2 = subsequence, -1 = no match. */
function matchRank(query: string, champ: ChampionInfo): number {
  let best = -1;
  for (const raw of [champ.name, champ.imageId, champ.nameJa]) {
    const target = normalizeForSearch(raw);
    if (!target) continue;
    if (target.startsWith(query)) return 0;
    if (best !== 1 && target.includes(query)) best = 1;
    else if (best === -1 && isSubsequence(query, target)) best = 2;
  }
  return best;
}

/** Developer-mode-only playground for experimenting with new UI before it
 * lands in the real overlay. Currently: champion search autocomplete built on
 * base-ui-solid's Input (the Solid port has no Autocomplete yet, so the
 * listbox part is hand-rolled). */
export function DevPlayground(props: { onClose: () => void }) {
  return (
    <div class="flex flex-col gap-3 min-h-0">
      <div class="flex items-center justify-between">
        <div class="font-hx-display text-[11px] font-bold tracking-[0.28em] text-hx-gold">
          UI PLAYGROUND
        </div>
        <button
          type="button"
          class="text-[10px] text-hx-muted hover:text-hx-gold cursor-pointer"
          onClick={() => props.onClose()}
        >
          ← 戻る
        </button>
      </div>
      <ChampionSearchDemo />
    </div>
  );
}

function ChampionSearchDemo() {
  const [query, setQuery] = createSignal("");
  const [open, setOpen] = createSignal(false);
  const [highlighted, setHighlighted] = createSignal(0);
  const [selected, setSelected] = createSignal<{ key: number } & ChampionInfo>();

  const matches = createMemo(() => {
    const q = normalizeForSearch(query());
    if (!q) return allChampions();
    return allChampions()
      .map((c) => ({ champ: c, rank: matchRank(q, c) }))
      .filter((m) => m.rank >= 0)
      .sort((a, b) => a.rank - b.rank || a.champ.name.localeCompare(b.champ.name))
      .map((m) => m.champ);
  });

  const pick = (champ: { key: number } & ChampionInfo) => {
    setSelected(champ);
    setQuery(champ.name);
    setOpen(false);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (!open() && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      setOpen(true);
      e.preventDefault();
      return;
    }
    if (e.key === "ArrowDown") {
      setHighlighted((i) => Math.min(i + 1, matches().length - 1));
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      setHighlighted((i) => Math.max(i - 1, 0));
      e.preventDefault();
    } else if (e.key === "Enter") {
      const champ = matches()[highlighted()];
      if (open() && champ) pick(champ);
      e.preventDefault();
    } else if (e.key === "Escape") {
      setOpen(false);
    }
  };

  return (
    <div class="flex flex-col gap-2">
      <span class="text-[11px] text-hx-muted">チャンピオン検索 (base-ui-solid Input)</span>
      <div class="relative">
        <Input
          value={query()}
          placeholder="Search champion…"
          class="w-full rounded border border-hx-border bg-hx-bg-raised px-2 py-1 text-[12px] text-hx-text outline-none focus:border-hx-gold placeholder:text-hx-muted"
          // Not onValueChange: the Solid port wires it to the DOM `change`
          // event (blur-time), so per-keystroke filtering needs onInput.
          onInput={(e: InputEvent & { currentTarget: HTMLInputElement }) => {
            setQuery(e.currentTarget.value);
            setHighlighted(0);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setOpen(false)}
          onKeyDown={onKeyDown}
        />
        <Show when={open()}>
          <div
            class="absolute z-10 mt-1 max-h-48 w-full overflow-y-auto rounded border border-hx-border bg-hx-panel p-1 shadow-lg"
            role="listbox"
          >
            <For
              each={matches()}
              fallback={<div class="px-2 py-1 text-[11px] text-hx-muted">該当なし</div>}
            >
              {(champ, index) => (
                <div
                  role="option"
                  tabIndex={-1}
                  aria-selected={index() === highlighted()}
                  class={`flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-[12px] ${
                    index() === highlighted()
                      ? "bg-hx-gold-wash text-hx-gold"
                      : "text-hx-text hover:bg-hx-gold-wash"
                  }`}
                  onMouseEnter={() => setHighlighted(index())}
                  onMouseDown={(e) => {
                    // Runs before the input's blur, so the click wins.
                    e.preventDefault();
                    pick(champ);
                  }}
                >
                  <img class="h-5 w-5 rounded" src={champIconByName(champ.imageId)} alt="" />
                  <span>{champ.name}</span>
                  <Show when={champ.nameJa}>
                    <span class="text-[10px] text-hx-muted">{champ.nameJa}</span>
                  </Show>
                  <span class="ml-auto text-[10px] text-hx-muted">#{champ.key}</span>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
      <Show when={selected()}>
        {(champ) => (
          <div class="flex items-center gap-2 rounded border border-hx-border bg-hx-bg-raised p-2">
            <img class="h-8 w-8 rounded" src={champIconByName(champ().imageId)} alt="" />
            <div class="flex flex-col">
              <span class="text-[12px] text-hx-gold">{champ().name}</span>
              <span class="text-[10px] text-hx-muted">
                id: {champ().key} / {champ().imageId}
              </span>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
