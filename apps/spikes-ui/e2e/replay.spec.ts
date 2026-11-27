import { expect, test } from "@playwright/test";
import { addBackground } from "./helpers";

test("record a run, replay it in the same views, seek, export and download", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "reward", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");

  // A recorded live run, with dopamine released partway.
  await addBackground(page, { neurons: "in-", hz: 20, mv: 6 });
  await page.getByTestId("build").click();
  await expect(page.getByText("3 neurons")).toBeVisible();
  await page.getByLabel(/Neurons \(up to/).fill("out");
  await page.getByRole("option", { name: "out", exact: true }).click();
  await page.keyboard.press("Escape");
  await page.getByTestId("record").check();
  await page.getByTestId("start").click();
  await page.waitForTimeout(800);
  await page.getByTestId("release-dopamine").click();
  await page.waitForTimeout(1200);
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");

  // The recording is listed; replay it.
  await page.getByTestId("nav-recordings").click();
  const recording = page.getByTestId("recording").first();
  await expect(recording).toContainText("spikes.arrows");
  await recording.getByTestId("replay").click();
  await expect(page.getByTestId("build")).toHaveText(/Load/);
  await expect(page.getByTestId("seek")).toBeVisible();
  await expect(page.getByTestId("edit-inputs")).toHaveCount(0); // a replay takes no input
  await page.getByTestId("build").click();
  await expect(page.getByText("3 neurons")).toBeVisible();
  await page.getByLabel(/Neurons \(up to/).fill("out");
  await page.getByRole("option", { name: "out", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("neuron-state")).toHaveCount(0); // no live neuron state in a replay
  await page.getByTestId("start").click();
  await expect(page.getByTestId("timing")).toContainText(/Spikes\s*[1-9][\d,]*\/s/, { timeout: 5000 });
  await expect(page.getByTestId("inspector").locator("canvas")).toHaveCount(2); // membrane and weights
  await page.waitForTimeout(1500);
  await page.screenshot({ path: "test-results/replay.png", fullPage: true });
  await page.getByTestId("stop").click();

  // Seek back to the start: click the slider's left end.
  await expect(page.getByTestId("state")).toHaveText("ready");
  const track = (await page.getByTestId("seek").locator("[data-slot=slider]").boundingBox())!;
  await page.mouse.click(track.x + 1, track.y + track.height / 2);
  await expect(page.getByTestId("timing")).toContainText(/Simulated time\s*0\.0/, { timeout: 5000 });

  // Export to Parquet, and download a table.
  await page.getByTestId("nav-recordings").click();
  await page.getByTestId("recording").first().getByRole("button", { name: "Export to Parquet" }).click();
  await expect(page.getByText(/Exported .* to Parquet/)).toBeVisible();
  await expect(page.getByTestId("recording").first()).toContainText("spikes.parquet");
  const href = (await page.getByTestId("recording").first().locator("a", { hasText: "spikes.parquet" }).getAttribute("href"))!;
  expect(href).toMatch(new RegExp(`^${baseURL}/api/recordings/[^/]+/spikes.parquet$`));
  const download = await fetch(href);
  expect(download.ok).toBe(true);
  expect((await download.arrayBuffer()).byteLength).toBeGreaterThan(100);
});
