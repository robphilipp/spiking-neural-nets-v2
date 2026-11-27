import { expect, type Locator, type Page, test } from "@playwright/test";

const look = async (chart: Locator) => (await chart.screenshot({ animations: "disabled" })).toString("base64");

/** Every live chart holds still: two pictures a second apart are the same. */
async function still(page: Page) {
  await page.mouse.move(0, 0);
  for (const id of ["raster", "rate", "weight-trend"]) {
    const chart = page.getByTestId(id);
    await expect
      .poll(async () => {
        const a = await look(chart);
        await page.waitForTimeout(1000);
        return a === (await look(chart));
      }, { message: id, timeout: 10_000 })
      .toBe(true);
  }
}

test("with cadence on, the charts stop scrolling when the run stops or pauses, and keep their zoom", async ({ page }) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.addStyleTag({ content: "header { position: static !important; }" });
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "pinned-nodes", exact: true }).click();
  await page.getByTestId("apply").click();
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Background neurons", { exact: true }).fill("sensors-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("50");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  await expect(page.getByText("220 neurons")).toBeVisible();
  await page.getByTestId("cadence").check();
  // A window shorter than the run, so that the axes scroll.
  await page.getByLabel("Time window").click();
  await page.getByRole("option", { name: "1 s", exact: true }).click();

  // Running: the charts move.
  await page.getByTestId("start").click();
  await page.waitForTimeout(2500);
  const rate = page.getByTestId("rate");
  const moving = await look(rate);
  await page.waitForTimeout(500);
  expect(await look(rate)).not.toBe(moving);

  // The charts' canvases, marked: a chart drawn anew (losing its zoom) would have a new one.
  const mark = () =>
    page.evaluate(() => {
      for (const id of ["raster", "rate", "weight-trend"]) (document.querySelector(`[data-testid="${id}"] canvas`) as HTMLElement).dataset.mark = "kept";
    });
  const kept = () =>
    page.evaluate(() =>
      ["raster", "rate", "weight-trend"].every((id) => (document.querySelector(`[data-testid="${id}"] canvas`) as HTMLElement | null)?.dataset.mark === "kept"),
    );
  await mark();

  await page.getByTestId("pause").click();
  await expect(page.getByTestId("state")).toHaveText("paused");
  await still(page);

  // A step while paused shows its data, then the charts hold still again.
  const raster = page.getByTestId("raster");
  const before = await look(raster);
  await page.getByRole("button", { name: "Step 10 ms" }).click();
  await expect.poll(() => look(raster)).not.toBe(before);
  await still(page);

  await page.getByRole("button", { name: "Resume" }).click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await page.waitForTimeout(2000);
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await still(page);
  expect(await kept()).toBe(true);
  await page.getByTestId("weights").screenshot({ path: "test-results/cadence-stopped.png" });
});
