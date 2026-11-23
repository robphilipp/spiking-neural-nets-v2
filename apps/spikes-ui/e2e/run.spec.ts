import { expect, test } from "@playwright/test";

test("open an example, build, stimulate and run it, and watch it live", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");

  // Network page: open the ring example and apply it.
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await expect(page.locator(".monaco-editor")).toContainText("hidden-101");
  await page.getByTestId("apply").click();
  await expect(page.getByText("Applied.")).toBeVisible();

  // Run page: build, stimulate group 1, run at real time.
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();
  await page.getByLabel("Stimulate neurons starting with").fill("hidden-10");
  await page.getByTestId("stimulate").click();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await expect(page.getByTestId("raster").locator("canvas")).toBeVisible();
  await expect(page.getByTestId("timing")).toContainText(/Frames\s*[1-9]\d\/s/, { timeout: 5000 });
  await page.screenshot({ path: "test-results/run-page.png", fullPage: true });

  await page.getByTestId("pause").click();
  await expect(page.getByTestId("state")).toHaveText("paused");
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  expect(errors).toEqual([]);
});

test("parse errors are shown in the editor", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.locator(".monaco-editor").click();
  await page.keyboard.type("(NRN=[(nid=a, mst=1 parsec)])");
  await page.getByTestId("apply").click();
  await expect(page.getByText(/error\(s\) in the description/)).toBeVisible();
  await expect(page.locator(".monaco-editor .squiggly-error").first()).toBeAttached();
});
