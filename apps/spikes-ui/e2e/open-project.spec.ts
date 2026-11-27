import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";

const ring = join(process.cwd(), "..", "..", "test-data", "networks", "ring.boo");

test("the app opens on the Project page, with the pages in order", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await expect(page.getByLabel("Project name")).toBeVisible();
  const order = await page.locator('[data-testid^="nav-"]').evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
  expect(order).toEqual(["nav-projects", "nav-network", "nav-inputs", "nav-run", "nav-recordings", "nav-deployment", "nav-settings"]);
});

test("opening a project asks which input file, and Run lands on the Run page built with it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  // Two input files: the default and a second one.
  await page.getByTestId("nav-inputs").click();
  // Many input files: the dialog's list scrolls.
  for (let i = 2; i <= 12; i++) {
    await page.getByTestId("file-new").click();
    await expect(page.getByLabel("Input file (used by runs)")).toHaveText(`inputs-${i}`);
  }

  await page.getByTestId("nav-projects").click();
  await page.getByTestId("file-input").setInputFiles(ring);
  const dialog = page.getByTestId("open-project-dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByTestId("open-project-input-inputs")).toBeVisible();
  const list = dialog.getByTestId("open-project-inputs");
  expect(await list.evaluate((e) => e.scrollHeight > e.clientHeight)).toBe(true); // scrolls
  await expect(dialog.getByTestId("open-project-input-inputs-12")).toBeInViewport(); // the selected one is shown
  await dialog.getByTestId("open-project-input-inputs").click(); // not the selected one
  await dialog.screenshot({ path: "test-results/open-project-dialog.png" });
  await dialog.getByTestId("open-project-run").click();

  await expect(page.getByTestId("state")).toHaveText("ready"); // the Run page, built
  await expect(page.getByText("25 neurons")).toBeVisible();
  await expect(page.getByLabel("Input file")).toHaveText("inputs");
  await expect(page.getByTestId("start")).toBeEnabled();
});

test("Edit network goes to the editor; Run with errors in the network goes there too, with the errors marked", async ({ page }, info) => {
  const broken = info.outputPath("broken.boo");
  writeFileSync(broken, "this is not a network\n");
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");

  await page.getByTestId("file-input").setInputFiles(ring);
  await page.getByTestId("open-project-edit").click();
  await expect(page.locator(".monaco-editor")).toContainText("hidden-101");

  await page.getByTestId("nav-projects").click();
  await page.getByTestId("file-input").setInputFiles(broken);
  await page.getByTestId("open-project-run").click();
  await expect(page.locator(".monaco-editor")).toContainText("this is not a network");
  await expect(page.getByText("Not run")).toBeVisible();
});
