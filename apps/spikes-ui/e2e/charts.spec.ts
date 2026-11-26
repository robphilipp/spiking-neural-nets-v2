import { expect, type Page, test } from "@playwright/test";

/** Spikes in the raster's data source, per row. */
const rasterRows = (page: Page) =>
  page.evaluate(() => {
    type Source = { seriesList: () => Array<{ name: string; data: unknown[] }> };
    const live = (window as unknown as { __spikes: { live: { getState: () => { view: { raster: { value: Source } } | null } } } }).__spikes.live;
    const view = live.getState().view;
    return Object.fromEntries((view?.raster.value.seriesList() ?? []).map((s) => [s.name, s.data.length]));
  });

const total = (rows: Record<string, number>) => Object.values(rows).reduce((a, b) => a + b, 0);

test("changing the raster's neurons keeps what it showed; chart settings apply and are remembered", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");

  // A run in which each of the ring's 25 neurons spikes once.
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();
  await page.getByLabel("Stimulate neurons starting with").fill("hidden-10");
  await page.getByTestId("stimulate").click();
  await page.getByTestId("start").click();
  await expect.poll(async () => total(await rasterRows(page)), { timeout: 10_000 }).toBe(25);
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  const all = await rasterRows(page);
  expect(Object.keys(all)).toHaveLength(25);

  // Fewer rows: the ones still shown keep their spikes.
  await page.getByLabel("Show neurons starting with").fill("hidden-10");
  await expect.poll(async () => Object.keys(await rasterRows(page)).length).toBeLessThan(25);
  const some = await rasterRows(page);
  for (const [name, n] of Object.entries(some)) expect(n, name).toBe(all[name]);
  expect(total(some)).toBeGreaterThan(0);

  // All rows again: every spike is back (the others' from the whole-network stream).
  await page.getByLabel("Show neurons starting with").fill("");
  await expect.poll(async () => Object.keys(await rasterRows(page)).length).toBe(25);
  expect(await rasterRows(page)).toEqual(all);

  // The chart settings: shown, applied, remembered.
  const controls = page.getByTestId("chart-controls");
  await expect(controls).toBeVisible();
  await page.getByTestId("raster").scrollIntoViewIfNeeded();
  await page.screenshot({ path: "test-results/chart-controls.png", fullPage: true });
  await page.getByTestId("buffering").fill("120");
  await page.getByTestId("update-rate").fill("80");
  await page.getByTestId("cadence").click();
  await page.getByTestId("keep").click();
  await page.getByRole("option", { name: "30 s", exact: true }).click();
  await expect(page.getByTestId("raster").locator("canvas")).toBeVisible();
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("spikes.charts") ?? "{}"));
  expect(saved).toEqual({ bufferMs: 120, updateMs: 80, cadence: true, keepMs: 30_000 });
  await page.getByRole("button", { name: "Reset the chart settings" }).click();
  await expect(page.getByTestId("buffering")).toHaveValue("50");
  await expect(page.getByTestId("cadence")).not.toBeChecked();
  // Remembered across reloads.
  await page.getByTestId("buffering").fill("75");
  await page.reload();
  await expect(page.getByTestId("connection")).toHaveText("connected");
  const reloaded = await page.evaluate(
    () => (window as unknown as { __spikes: { charts: { getState: () => { bufferMs: number } } } }).__spikes.charts.getState().bufferMs,
  );
  expect(reloaded).toBe(75);
  expect(errors).toEqual([]);
});

/** Points in the inspector's membrane data source, per neuron. */
const membraneRows = (page: Page) =>
  page.evaluate(() => {
    type Source = { seriesList: () => Array<{ name: string; data: unknown[] }> };
    const inspect = (window as unknown as { __spikes: { inspect: { getState: () => { view: { membrane: { value: Source } } | null } } } }).__spikes
      .inspect;
    const view = inspect.getState().view;
    return Object.fromEntries((view?.membrane.value.seriesList() ?? []).map((s) => [s.name, s.data.length]));
  });

test("inspecting another neuron keeps the traces already shown", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();

  const picker = page.getByTestId("inspect-picker").getByRole("combobox");
  await picker.fill("hidden-101");
  await page.getByRole("option", { name: "hidden-101", exact: true }).click();
  await page.getByLabel("Stimulate neurons starting with").fill("hidden-10");
  await page.getByTestId("stimulate").click();
  await page.getByTestId("start").click();
  await expect.poll(async () => (await membraneRows(page))["hidden-101"] ?? 0, { timeout: 10_000 }).toBeGreaterThan(2);
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  const before = (await membraneRows(page))["hidden-101"]!;

  // A second neuron: the first one's trace stays.
  await picker.fill("hidden-201");
  await page.getByRole("option", { name: "hidden-201", exact: true }).click();
  await expect.poll(async () => Object.keys(await membraneRows(page))).toEqual(["hidden-101", "hidden-201"]);
  expect((await membraneRows(page))["hidden-101"]).toBe(before);
});
