import { describe, expect, it } from "vitest";
import { normalizeForSearch, searchChampions } from "./championSearch";

const champs = [
  { name: "Ahri", imageId: "Ahri", nameJa: "アーリ" },
  { name: "Naafiri", imageId: "Naafiri", nameJa: "ナーフィリ" },
  { name: "Bel'Veth", imageId: "Belveth", nameJa: "ベル＝ヴェス" },
  { name: "Kai'Sa", imageId: "Kaisa", nameJa: "カイ＝サ" },
  { name: "Wukong", imageId: "MonkeyKing", nameJa: "ウーコン" },
];

describe("normalizeForSearch", () => {
  it("folds katakana into hiragana and strips separators", () => {
    expect(normalizeForSearch("ナーフィリ")).toBe(normalizeForSearch("なーふぃり"));
    expect(normalizeForSearch("Kai Sa")).toBe("kaisa");
    expect(normalizeForSearch("Bél'Veth")).toBe("belveth");
  });
});

describe("searchChampions", () => {
  const names = (q: string) => searchChampions(champs, q).map((c) => c.name);

  it("returns everything for an empty query", () => {
    expect(names("")).toHaveLength(champs.length);
  });

  it("matches Japanese names", () => {
    expect(names("あーり")).toEqual(["Ahri"]);
    expect(names("ナーフ")).toEqual(["Naafiri"]);
  });

  it("matches by image id", () => {
    expect(names("monkey")).toEqual(["Wukong"]);
  });

  it("matches subsequences and ranks prefixes first", () => {
    expect(names("nafi")).toEqual(["Naafiri"]);
    expect(names("ka")[0]).toBe("Kai'Sa");
  });

  it("ignores apostrophes and spaces", () => {
    expect(names("belveth")).toEqual(["Bel'Veth"]);
    expect(names("kai sa")).toEqual(["Kai'Sa"]);
  });
});
