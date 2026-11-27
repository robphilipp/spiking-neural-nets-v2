import { expect, test } from "@playwright/test";
import { addRealtime } from "./helpers";

test("the 3D view shows sensors and what they drive", async ({ page }) => {
  test.setTimeout(60_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "line-sensor-layers-wta", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  // A microphone source on the 49 input neurons: while the run goes, it's a sensor.
  await addRealtime(page, "microphone", { name: "voice", neurons: "input-" });
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();
  await expect(page.getByTestId("sensor-legend")).toHaveCount(0);
  await page.getByTestId("start").click();

  const legend = page.getByTestId("sensor-legend");
  await expect(legend).toContainText("voice · 49 channels", { timeout: 5000 });
  await expect(legend.locator(".bg-\\[\\#009e73\\]")).toHaveCount(1, { timeout: 5000 }); // its data is flowing
  await expect(page.getByText(/Green diamonds are sensors/)).toBeVisible();
  await page.waitForTimeout(800);
  await page.getByTestId("network-3d").screenshot({ path: "test-results/sensors-3d.png" });

  // Ending the run disconnects it.
  await page.getByTestId("stop").click();
  await expect(legend).toHaveCount(0, { timeout: 5000 });
});
