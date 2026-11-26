import { expect, type Locator, type Page, test } from "@playwright/test";

const look = async (chart: Locator) => (await chart.screenshot({ animations: "disabled" })).toString("base64");
const CHARTS = ["raster", "rate", "weight-trend"];
const simTime = async (page: Page) => Number(/Simulated time\s*([\d.]+) s/.exec(await page.getByTestId("timing").innerText())![1]);

test("frozen charts hold still while the run goes on, and catch up when unfrozen", async ({ page }) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.addStyleTag({ content: "header { position: static !important; }" });
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "wta-learning", exact: true }).click();
  await page.getByTestId("apply").click();
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Neurons starting with", { exact: true }).fill("in-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("50");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await page.getByLabel("Time window").click();
  await page.getByRole("option", { name: "5 s", exact: true }).click();
  await page.getByTestId("start").click();
  await page.waitForTimeout(2000);
  await page.mouse.move(0, 0);

  await page.getByTestId("freeze").check();
  await expect(page.getByTestId("frozen-note")).toBeVisible();
  await page.waitForTimeout(300);
  const frozen = await Promise.all(CHARTS.map((id) => look(page.getByTestId(id))));
  const t0 = await simTime(page);
  await page.waitForTimeout(2000);
  // The run went on (and its numbers with it); the charts didn't move.
  expect(await simTime(page)).toBeGreaterThan(t0 + 1);
  await expect(page.getByTestId("state")).toHaveText("running");
  for (const [i, id] of CHARTS.entries()) expect(await look(page.getByTestId(id)), id).toBe(frozen[i]);

  // Zoom works while frozen, and nothing scrolls it away.
  const raster = page.getByTestId("raster");
  const box = (await raster.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.8, box.y + box.height / 3);
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, -300);
  await page.keyboard.up("Shift");
  await page.mouse.move(0, 0);
  await expect.poll(() => look(raster)).not.toBe(frozen[0]);
  const zoomed = await look(raster);
  await page.waitForTimeout(1000);
  expect(await look(raster)).toBe(zoomed);

  // Stopped and unfrozen: the charts take in what came meanwhile, and end where the run did.
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await page.getByTestId("reset-zoom").click();
  await page.getByTestId("freeze").uncheck();
  await expect(page.getByTestId("frozen-note")).toHaveCount(0);
  for (const [i, id] of CHARTS.entries()) await expect.poll(() => look(page.getByTestId(id)), { message: id }).not.toBe(frozen[i]);
  await page.waitForTimeout(500);
  await page.getByTestId("rate").screenshot({ path: "test-results/freeze-rate.png" });
  await page.getByTestId("raster").screenshot({ path: "test-results/freeze-raster.png" });
});
