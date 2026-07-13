import { expect, test } from "@playwright/test";

const mockUrl = (extra = "") =>
  `/?desktop-test&player-stats-mock${extra ? `&${extra}` : ""}#/summoners`;

async function search(page: import("@playwright/test").Page, riotId: string) {
  await page.getByLabel("Riot ID").fill(riotId);
  await page.getByRole("button", { name: "検索", exact: true }).click();
}

test("auto-searches the current player and supports provider, details, load more, and refresh", async ({
  page,
}) => {
  await page.goto(mockUrl("current-player=Auto%20Player%23JP1"));
  await expect(page.getByRole("heading", { name: "Auto Player#JP1" })).toBeVisible();
  await expect(page.locator(".summoner-match")).toHaveCount(20);

  await page.getByLabel("Provider").selectOption("ugg");
  await expect(page.getByLabel("Provider")).toHaveValue("ugg");
  await expect(page.getByRole("heading", { name: "Auto Player#JP1" })).toBeVisible();

  await page.locator(".summoner-match summary").first().click();
  await expect(page.locator(".summoner-match").first().locator(".summoner-participants > div")).toHaveCount(10);

  await page.getByRole("button", { name: "さらに20件読み込む" }).click();
  await expect(page.locator(".summoner-match")).toHaveCount(40);

  await page.getByRole("button", { name: "再読み込み" }).click();
  await expect(page.getByRole("heading", { name: "Auto Player#JP1" })).toBeVisible();
});

test("handles arbitrary search, queue filters, partial retry, and history deletion", async ({ page }) => {
  await page.goto(mockUrl());
  await search(page, "Partial#JP1");
  await expect(page.getByText("1件の詳細取得に失敗しました。")).toBeVisible();
  await expect(page.getByRole("button", { name: "失敗分を再試行" })).toBeVisible();

  await page.getByLabel("キューで試合を絞り込む").selectOption("440");
  await expect(page.locator(".summoner-match")).toHaveCount(5);

  await page.getByRole("button", { name: "Partialの履歴を削除" }).click();
  await expect(page.getByRole("button", { name: /JP1Partial#JP1/ })).toHaveCount(0);
});

test("renders not-found and rate-limit states", async ({ page }) => {
  await page.goto(mockUrl());
  await search(page, "Missing#JP1");
  await expect(page.getByRole("alert")).toContainText("サモナーが見つかりません");

  await search(page, "Limited#JP1");
  await expect(page.getByRole("alert")).toContainText("再試行まで約 30 秒です。");
});

test("restores the last search and preserves usable responsive controls", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem(
      "lol-overlay.player-search-history.v1",
      JSON.stringify([{ platformId: "NA1", gameName: "Restored", tagLine: "NA1" }]),
    );
  });
  await page.setViewportSize({ width: 600, height: 800 });
  await page.goto(mockUrl());
  await expect(page.getByRole("heading", { name: "Restored#NA1" })).toBeVisible();
  await expect(page.getByLabel("Riot ID")).toHaveCSS("font-size", "16px");
  await expect(page.getByRole("button", { name: "検索", exact: true })).toBeVisible();
});
