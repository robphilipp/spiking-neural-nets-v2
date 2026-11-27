import { expect, test } from "@playwright/test";
import { addBackground, addPulse } from "./helpers";

test("the Run page describes the selected input file; the Inputs page shows unsaved changes", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();

  await addPulse(page, { name: "tap", neurons: "hidden-10", mv: 15 });
  await addBackground(page, { neurons: "hidden-3", hz: 7, mv: 9 });
  const summary = page.getByTestId("input-description");
  await expect(summary).toContainText("tap: 15 mV pulse to names starting with");
  await expect(summary).toContainText("random pulses, 7 Hz each, 9 mV");

  // Edited on the Inputs page: unsaved there too (the browser can't write, so no Save button).
  await page.getByTestId("nav-inputs").click();
  await expect(page.getByTestId("inputs-unsaved")).toBeVisible();
});
