/** Champion-name matching shared by every search box (tier list, matchup
 * combobox). Ported from the old developer-mode UI playground. */

export interface ChampionSearchable {
  /** Display name, e.g. "Wukong". */
  name: string;
  /** Image id used in icon filenames, e.g. "MonkeyKing". */
  imageId: string;
  /** Japanese display name ("" if unavailable). */
  nameJa: string;
}

/** Lowercase, strip diacritics/punctuation/spaces, fold katakana (incl.
 * half-width) into hiragana — so "kai sa", "ナーフィリ" and "なーふぃり" all
 * normalize to comparable strings. */
export function normalizeForSearch(s: string): string {
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

/** Match rank for a pre-normalized query: 0 = prefix, 1 = substring,
 * 2 = subsequence, -1 = no match. */
export function matchRank(query: string, champ: ChampionSearchable): number {
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

/** Filter + rank `list` by `rawQuery` (best matches first, ties by name).
 * An empty query returns the list unchanged. */
export function searchChampions<T extends ChampionSearchable>(list: T[], rawQuery: string): T[] {
  const query = normalizeForSearch(rawQuery.trim());
  if (!query) return list;
  return list
    .map((champ) => ({ champ, rank: matchRank(query, champ) }))
    .filter((m) => m.rank >= 0)
    .sort((a, b) => a.rank - b.rank || a.champ.name.localeCompare(b.champ.name))
    .map((m) => m.champ);
}
