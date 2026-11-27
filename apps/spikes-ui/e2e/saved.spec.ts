import { expect, test } from "@playwright/test";
import { addBackground } from "./helpers";

test("save a run, then continue it from the Recordings page", async ({ page }) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "reward", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");

  // Run with background input for a while, stop, save.
  await addBackground(page, { neurons: "in-", hz: 20, mv: 6 });
  await page.getByTestId("build").click();
  await expect(page.getByText("3 neurons")).toBeVisible();
  await expect(page.getByTestId("save-run")).toBeEnabled();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("save-run")).toBeDisabled(); // pause (or stop) first
  await page.waitForTimeout(1000);
  await page.getByTestId("pause").click();
  await expect(page.getByTestId("state")).toHaveText("paused");
  await page.getByTestId("save-run").click(); // saved where it waits; it stays paused
  await expect(page.getByTestId("state")).toHaveText("paused");
  await expect(page.getByTestId("saved-note")).toContainText(/Saved at \d+\.\d\d s/);
  const savedAt = Number((await page.getByTestId("saved-note").textContent())!.match(/Saved at (\d+\.\d\d)/)![1]);
  expect(savedAt).toBeGreaterThan(0.3);
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");

  // It's listed with the recordings; continue it: a new session, ready at the saved time.
  await page.getByTestId("nav-recordings").click();
  const saved = page.getByTestId("saved-run").first();
  await expect(saved).toContainText(`at ${savedAt.toFixed(2)} s`);
  await saved.getByTestId("continue-run").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await expect(page.getByText("3 neurons")).toBeVisible();
  const simulated = async () => Number((await page.getByTestId("timing").textContent())!.match(/Simulated time\s*([\d.]+) s/)![1]);
  await expect.poll(simulated, { timeout: 5000 }).toBeCloseTo(savedAt, 1); // shown to 0.1 s
  await page.getByTestId("start").click();
  await expect(page.getByTestId("timing")).toContainText(/Spikes\s*[1-9][\d,]*\/s/, { timeout: 5000 });
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
});
