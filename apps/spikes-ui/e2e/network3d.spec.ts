import { expect, test } from "@playwright/test";

test("the 3D view draws the network, flashes spikes, and selects a region for the raster", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByText("Applied.")).toBeVisible();
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Neurons starting with", { exact: true }).fill("hidden-1");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByLabel("mV", { exact: true }).nth(1).fill("10");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();

  const view = page.getByTestId("network-3d");
  await expect(view.locator("canvas")).toBeVisible();
  await page.waitForTimeout(300);
  await view.screenshot({ path: "test-results/network-3d-idle.png" });
  await page.getByTestId("start").click();
  await page.waitForTimeout(1500);
  await view.screenshot({ path: "test-results/network-3d.png" });

  // Shift-drag across the whole view: every neuron is in the region, and becomes a raster row.
  const b = (await view.boundingBox())!;
  await page.keyboard.down("Shift");
  await page.mouse.move(b.x + 2, b.y + 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 5 });
  await page.mouse.move(b.x + b.width - 2, b.y + b.height - 2, { steps: 5 });
  await page.mouse.up();
  await page.keyboard.up("Shift");
  await expect(page.getByTestId("region")).toContainText("Region: 25 neurons");
  await expect(page.getByText("Spikes (")).toHaveCount(0); // all 25 shown, not "(n of 25)"

  // Clearing the region goes back to the name filter.
  await page.getByRole("button", { name: "Clear the region" }).click();
  await expect(page.getByTestId("region")).toHaveCount(0);
  await page.getByTestId("stop").click();
});
