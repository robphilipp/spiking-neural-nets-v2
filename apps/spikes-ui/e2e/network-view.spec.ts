import { expect, test } from "@playwright/test";

test("the Network page's bottom panel builds and shows the network in 3D, and resizes", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  const panel = page.getByTestId("network-view-panel");
  const toggle = page.getByTestId("network-view-toggle");
  if ((await toggle.getAttribute("aria-expanded")) === "true") await toggle.click();
  await expect(panel.getByTestId("network-3d")).toHaveCount(0);

  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByText("Applied.")).toBeVisible();
  await expect(page.getByTestId("network-view-status")).toHaveText("Not built yet");

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await page.getByTestId("network-view-build").click();
  await expect(page.getByTestId("network-view-status")).toContainText("25 neurons");
  const view = panel.getByTestId("network-3d");
  await expect(view.locator("canvas")).toBeVisible();
  await page.waitForTimeout(300);
  await panel.screenshot({ path: "test-results/network-view.png" });

  // Dragging the top edge up makes the view taller.
  const before = (await view.boundingBox())!.height;
  const edge = (await page.getByTestId("network-view-resize").boundingBox())!;
  await page.mouse.move(edge.x + edge.width / 2, edge.y + edge.height / 2);
  await page.mouse.down();
  await page.mouse.move(edge.x + edge.width / 2, edge.y - 80, { steps: 5 });
  await page.mouse.up();
  await expect.poll(async () => (await view.boundingBox())!.height).toBeGreaterThan(before + 40);

  // The Run page sees the same build.
  await page.getByTestId("nav-run").click();
  await expect(page.getByText("25 neurons")).toBeVisible();

  // Collapsing hides the view.
  await page.getByTestId("nav-network").click();
  await toggle.click();
  await expect(panel.getByTestId("network-3d")).toHaveCount(0);
});
