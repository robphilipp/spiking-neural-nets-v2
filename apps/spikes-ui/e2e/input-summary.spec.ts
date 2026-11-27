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
  await expect(page.getByTestId("file-unsaved")).toBeVisible();
});

test("the Inputs page's file buttons: Save downloads in a browser, Add to project needs an open project, Rename renames", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-inputs").click();
  await expect(page.getByTestId("file-membership")).toHaveText("in the project");
  await expect(page.getByTestId("file-add-to-project")).toBeDisabled();

  await page.getByTestId("file-description").fill("changed");
  await expect(page.getByTestId("file-unsaved")).toBeVisible();
  const download = page.waitForEvent("download");
  await page.getByTestId("file-save").click();
  expect((await download).suggestedFilename()).toBe("inputs.inputs.toml");
  await expect(page.getByTestId("file-saved")).toBeVisible();

  await page.getByTestId("file-name").fill("protocol");
  await page.getByTestId("file-rename").click();
  await expect(page.getByLabel("Input file (used by runs)")).toHaveText("protocol");
});

test("Save acts on a name that was typed but not yet renamed", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("file-duplicate").click();
  await page.getByTestId("file-name").fill("typed-name"); // no Enter, no Rename
  const download = page.waitForEvent("download");
  await page.getByTestId("file-save").click();
  expect((await download).suggestedFilename()).toBe("typed-name.inputs.toml");
  await expect(page.getByLabel("Input file (used by runs)")).toHaveText("typed-name");
});
