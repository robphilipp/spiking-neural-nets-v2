import { expect, test } from "@playwright/test";

test("the weights panel follows learning across the whole network", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "line-sensor-layers-wta", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByText("Applied.")).toBeVisible();
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Neurons starting with", { exact: true }).fill("input-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("40");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();

  const panel = page.getByTestId("weights");
  await expect(panel).toContainText("Synapse weights");
  await expect(panel).toContainText("871"); // every synapse is listed
  await expect(panel).toContainText("No weight has changed yet");
  await expect(panel.getByTestId("weight-histogram")).toBeVisible();

  await page.getByTestId("start").click();
  // STDP changes weights as the network runs; the panel counts them and the histogram spreads.
  await expect(panel).not.toContainText("No weight has changed yet", { timeout: 10_000 });
  const changed = panel.getByText("Changed since build").locator("..");
  await expect(changed).toContainText(/[1-9]/);
  await expect(panel.getByTestId("weight-trend").locator("canvas, svg").first()).toBeVisible();
  await page.waitForTimeout(1500);
  await page.screenshot({ path: "test-results/shadcn-run.png", fullPage: true });
  await page.getByTestId("stop").click();

  // Dark mode follows the setting.
  await page.getByTestId("nav-settings").click();
  await page.getByLabel("Dark").check();
  await expect(page.locator("html")).toHaveClass(/dark/);
  await page.getByTestId("nav-run").click();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("raster").locator("canvas, svg").first()).toBeVisible();
  await page.waitForTimeout(1500);
  await page.screenshot({ path: "test-results/shadcn-run-dark.png", fullPage: true });
  await page.getByTestId("stop").click();
});
