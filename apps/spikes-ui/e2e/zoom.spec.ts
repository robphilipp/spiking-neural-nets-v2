import { expect, type Locator, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";
import { addBackground } from "./helpers";

/** The weight histogram's axis labels (it is SVG). */
async function ticks(chart: Locator): Promise<string> {
  return (await chart.locator("text").allTextContents()).join(" ");
}

/** What a stream chart shows (it draws on canvas, axes too), as a picture to compare. */
async function look(chart: Locator): Promise<string> {
  return (await chart.screenshot({ animations: "disabled" })).toString(
    "base64",
  );
}

async function shiftScroll(page: Page, chart: Locator, deltaY: number) {
  const box = (await chart.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 3);
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, deltaY);
  await page.keyboard.up("Shift");
}

async function drag(page: Page, chart: Locator, dx: number) {
  const box = (await chart.boundingBox())!;
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 3];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx / 2, y, { steps: 5 });
  await page.mouse.move(x + dx, y, { steps: 5 });
  await page.mouse.up();
}

test("every plot pans and zooms, and Reset zoom puts them back", async ({
  page,
  baseURL,
}) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  // Screenshots of a chart would otherwise depend on how far the page is scrolled: the sticky header covers it.
  await page.addStyleTag({
    content: "header { position: static !important; }",
  });
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page
    .getByRole("option", { name: "line-sensor-layers-wta", exact: true })
    .click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await addBackground(page, { neurons: "input-", hz: 40 });
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();
  await page.getByLabel("Run for (s)").fill("25");
  await page.getByLabel("Run for (s)").blur();
  await page.getByLabel("As fast as possible").check();
  const session = Number(
    /session (\d+)/.exec(
      await page.getByTestId("session-picker").innerText(),
    )![1],
  );
  await page.getByTestId("start").click();
  // Wait for the run to end (the state chip reads "ready" before it starts, too), then for the charts to settle.
  const client = await WebSocketClient.connect(baseURL!);
  await expect
    .poll(
      async () => (await client.listSessions()).find((s) => s.id === session),
      { timeout: 20_000 },
    )
    .toMatchObject({ state: "ready", simMs: 25000 });
  client.close();
  await page.mouse.move(0, 0);
  await expect
    .poll(async () => {
      const a = await look(page.getByTestId("raster"));
      await page.waitForTimeout(500);
      return a === (await look(page.getByTestId("raster")));
    })
    .toBe(true);

  // The raster: shift + scroll zooms, dragging pans, Reset zoom restores. (The run has ended, so nothing else moves.)
  const raster = page.getByTestId("raster");
  const start = await look(raster);
  expect(start).toBe(await look(raster)); // still
  await shiftScroll(page, raster, -400);
  await expect.poll(() => look(raster)).not.toBe(start);
  const zoomed = await look(raster);
  await drag(page, raster, 200);
  await expect.poll(() => look(raster)).not.toBe(zoomed);
  // Scrolling without shift scrolls the page, not the chart.
  // (Pictures taken with the mouse away from the chart: hovering highlights a row.)
  await page.mouse.move(0, 0);
  const before = await look(raster);
  const box = (await raster.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 3);
  await page.mouse.wheel(0, 100);
  await page.waitForTimeout(300);
  await page.mouse.move(0, 0);
  await expect.poll(() => look(raster)).toBe(before);
  await page.getByTestId("reset-zoom").click();
  await expect.poll(() => look(raster)).toBe(start);

  // The other live charts take the same gestures.
  for (const id of ["rate", "weight-trend"]) {
    const chart = page.getByTestId(id);
    await chart.scrollIntoViewIfNeeded();
    const was = await look(chart);
    await shiftScroll(page, chart, -400);
    await expect.poll(() => look(chart), { message: id }).not.toBe(was);
  }

  // The weight histogram (drawn by the app) too.
  const histogram = page.getByTestId("weight-histogram");
  await histogram.scrollIntoViewIfNeeded();
  const unzoomed = await ticks(histogram);
  expect(unzoomed).toContain("weight");
  await shiftScroll(page, histogram, -400);
  await expect.poll(() => ticks(histogram)).not.toBe(unzoomed);
  const zoomedIn = await ticks(histogram);
  await drag(page, histogram, -150);
  await expect.poll(() => ticks(histogram)).not.toBe(zoomedIn);
  await page
    .getByTestId("chart-controls")
    .screenshot({ path: "test-results/chart-controls.png" });
  await page.getByTestId("reset-zoom").click();
  await expect.poll(() => ticks(histogram)).toBe(unzoomed);

  // And the Inputs page's preview.
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-signal").click();
  const preview = page.getByTestId("preview");
  await expect(preview.locator("canvas").first()).toBeVisible();
  await page.waitForTimeout(300);
  const plain = await look(preview);
  await shiftScroll(page, preview, -400);
  await expect.poll(() => look(preview)).not.toBe(plain);
  await preview.getByTestId("reset-zoom").click();
  await expect.poll(() => look(preview)).toBe(plain);
});
