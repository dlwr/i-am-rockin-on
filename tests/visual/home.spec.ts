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
    await expect(cards).toHaveCount(4);

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

  test("Selector pick card shows 記事 affordance", async ({ page }) => {
    // Aldous Harding (in-window pick) は multi-source なので <details><summary>記事</summary>
    // が出る。単 source 時は <a>記事</a>。 どちらでも 「記事」 テキストが見えれば良い。
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/");
    await page.getByRole("button", { name: "Selector" }).click();
    const pick = page.locator("article.bg-card.shadow-zine.p-4");
    await expect(pick).toBeVisible();
    await expect(pick.getByText("記事")).toBeVisible();
  });

  test("multi-source dropdown stays within viewport on mobile (375px)", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 800 });
    await page.goto("/");

    // Aldous Harding が multi-source + YouTube で 左カラムに来るので、
    // pill が flex-wrap で折り返して <details> が card 左端に到達する。
    // これで dropdown が画面外に滑り落ちる経路を踏む。
    const grid = page.locator("ul.tilt-cycle");
    const aldous = grid.locator("> li", { hasText: "Aldous Harding" });
    await expect(aldous).toBeVisible();

    await aldous.locator("summary", { hasText: "記事" }).click();
    const dropdown = aldous.locator("details[open] > ul");
    await expect(dropdown).toBeVisible();

    const box = await dropdown.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(375);
  });

  test("multi-source dropdown overlays sibling cards on mobile (375px)", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 800 });
    await page.goto("/");

    const grid = page.locator("ul.tilt-cycle");
    const aldous = grid.locator("> li", { hasText: "Aldous Harding" });
    await aldous.locator("summary", { hasText: "記事" }).click();
    const dropdown = aldous.locator("details[open] > ul");
    await expect(dropdown).toBeVisible();

    // dropdown の中心 (画面座標) に何が描画されているかを elementsFromPoint で見て、
    // 一番上が dropdown 自身か dropdown の子孫であることを確認する。
    // sibling card (row 2 左の Bon Iver) が手前にあると elementsFromPoint[0] は
    // dropdown 外の要素になる。
    const box = await dropdown.boundingBox();
    const topmost = await page.evaluate(
      ([x, y]) => {
        const els = document.elementsFromPoint(x, y);
        const dd = document.querySelector("details[open] > ul");
        if (!dd || !els[0]) return "missing";
        return dd === els[0] || dd.contains(els[0]) ? "ok" : (els[0].tagName + "." + els[0].className);
      },
      [box!.x + box!.width / 2, box!.y + box!.height / 2],
    );
    expect(topmost).toBe("ok");
  });
});
