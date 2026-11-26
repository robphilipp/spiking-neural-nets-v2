import { expect, test } from "@playwright/test";

test("the Network page's badge says whether the network is applied, and applying reports in a toast", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  const status = page.getByTestId("network-status");
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await expect(status).toHaveText("Not applied");
  // The example list shows the example chosen.
  await expect(page.getByLabel("Open an example")).toHaveText("ring");

  // Applying: a toast (over the page: the editor doesn't move), and the badge.
  const editor = page.locator(".monaco-editor");
  const before = (await editor.boundingBox())!.y;
  await page.getByTestId("apply").click();
  const toast = page.locator("[data-sonner-toast]:not([data-removed='true'])").filter({ hasText: "Applied." });
  await expect(toast).toBeVisible();
  await expect(toast).toContainText("build and start it on the Run page");
  await expect(status).toHaveText("Applied");
  expect((await editor.boundingBox())!.y).toBe(before);
  await page.waitForTimeout(600); // (the toast slides in; the badge's colour fades)
  await page.screenshot({ path: "test-results/applied-toast.png" });

  // Editing the applied network: modified, until applied again.
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("\n// a comment");
  await expect(status).toHaveText("Modified");
  await page.waitForTimeout(600);
  await page.screenshot({ path: "test-results/modified-badge.png", clip: { x: 180, y: 50, width: 1220, height: 70 } });
  await page.getByTestId("apply").click();
  await expect(status).toHaveText("Applied");

  // One toast per apply (the new one replaces the last); its action goes to the Run page.
  await expect(page.locator("[data-sonner-toast]:not([data-removed='true'])")).toHaveCount(1);
  await page.locator("[data-sonner-toast]:not([data-removed='true'])").filter({ hasText: "Applied." }).getByRole("button", { name: "Go to Run" }).click();
  await expect(page.getByTestId("build")).toBeVisible();
  // (and leaving the Network page dismisses it)
  await expect(page.locator("[data-sonner-toast]:not([data-removed='true'])")).toHaveCount(0);

  // A parse error: an error toast, and still modified.
  await page.getByTestId("nav-network").click();
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("\n(oops");
  await page.getByTestId("apply").click();
  await expect(page.locator("[data-sonner-toast]:not([data-removed='true'])").filter({ hasText: "in the description" })).toContainText("Not applied");
  await expect(status).toHaveText("Modified");

  // Another network (an example chosen): not applied, though the engine has the earlier one.
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "simple-wta", exact: true }).click();
  await expect(status).toHaveText("Not applied");
  await page.getByTestId("apply").click();
  await expect(status).toHaveText("Applied");
});
