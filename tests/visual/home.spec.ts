import { test, expect } from "@playwright/test";

async function gridColumnCount(grid: import("@playwright/test").Locator): Promise<number> {
  // grid-template-columns computes to a space-separated list of pixel values.
  // Counting those tokens gives the actual rendered column count, which is
  // what we care about (independent of Tailwind class names).
  const cols = await grid.evaluate((el) => getComputedStyle(el).gridTemplateColumns);
  return cols.trim().split(/\s+/).filter(Boolean).length;
}

test.describe("home page visual regression (pre-seeded)", () => {
  test("renders 2-column grid on mobile (375px)", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 800 });
    await page.goto("/");

    const grid = page.locator("ul.tilt-cycle");
    await expect(grid).toBeVisible();
    await expect(grid).toHaveCSS("display", "grid");

    const cards = grid.locator("> li");
    await expect(cards).toHaveCount(3);

    expect(await gridColumnCount(grid)).toBe(2);
  });

  test("renders 3-column grid on tablet (768px)", async ({ page }) => {
    await page.setViewportSize({ width: 768, height: 1024 });
    await page.goto("/");
    const grid = page.locator("ul.tilt-cycle");
    await expect(grid).toBeVisible();
    expect(await gridColumnCount(grid)).toBe(3);
  });

  test("renders 4-column grid on desktop (1280px)", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/");
    const grid = page.locator("ul.tilt-cycle");
    await expect(grid).toBeVisible();
    expect(await gridColumnCount(grid)).toBe(4);
  });

  test("placeholder card (Spotify image None) has aspect-ratio 1/1", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/");
    const placeholder = page.locator("ul.tilt-cycle > li").first().locator("div[aria-hidden='true']");
    await expect(placeholder).toBeVisible();
    await expect(placeholder).toHaveCSS("aspect-ratio", "1 / 1");
  });

  test("Selector pick card shows 記事 link", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/");
    await page.getByRole("button", { name: "Selector" }).click();
    const pick = page.locator("article.bg-card.shadow-zine.p-4");
    await expect(pick).toBeVisible();
    await expect(pick.getByRole("link", { name: "記事" })).toBeVisible();
  });
});
