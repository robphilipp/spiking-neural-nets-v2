import { expect, test } from "@playwright/test";

test("the Network page's 3D view previews the text as written, without applying it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  const panel = page.getByTestId("network-view-panel");
  const toggle = page.getByTestId("network-view-toggle");
  if ((await toggle.getAttribute("aria-expanded")) === "true") await toggle.click();
  await expect(panel.getByTestId("network-3d")).toHaveCount(0);

  // Not applied: Update previews the editor's text all the same, and opens the panel.
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "pinned-nodes", exact: true }).click();
  await page.getByTestId("network-view-update").click();
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByTestId("network-view-status")).toHaveText("220 neurons · 3,000 connections");
  const view = panel.getByTestId("network-3d");
  await expect(view.locator("canvas")).toBeVisible();
  await page.waitForTimeout(300);
  await panel.screenshot({ path: "test-results/network-view.png" });

  // The tooltip: population, pin, and connections in and out. The sensors sit on a line along the bottom: find one.
  const b = (await view.boundingBox())!;
  const tooltip = page.getByTestId("neuron-tooltip");
  found: for (let y = b.y + b.height - 30; y > b.y + b.height / 2; y -= 4) {
    for (let x = b.x + b.width * 0.3; x < b.x + b.width * 0.7; x += 6) {
      await page.mouse.move(x, y);
      if (await tooltip.filter({ hasText: "sensors-" }).count()) break found;
    }
  }
  await expect(tooltip).toContainText("sensors-");
  await expect(tooltip).toContainText("node edge");
  await expect(page.getByTestId("tooltip-incoming")).toHaveText("none");
  await expect(page.getByTestId("tooltip-outgoing")).toContainText("excitatory");
  await panel.screenshot({ path: "test-results/network-view-tooltip.png" });

  // Nothing was applied or built.
  await expect(page.getByText("(changed)")).toHaveCount(0);
  await page.getByTestId("nav-run").click();
  await expect(page.getByText("Open or write a network on the Network page and apply it first.")).toBeVisible();

  // Editing marks the preview as out of date; a parse error shows in the status.
  await page.getByTestId("nav-network").click();
  await expect(page.getByTestId("network-view-status")).toContainText("220 neurons");
  await page.locator(".monaco-editor").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("(");
  await expect(page.getByTestId("network-view-status")).toContainText("the text has changed since");
  await page.getByTestId("network-view-update").click();
  await expect(page.getByTestId("network-view-status")).toContainText("error(s) in the description");

  // Dragging the top edge up makes the view taller.
  const before = (await view.boundingBox())!.height;
  const edge = (await page.getByTestId("network-view-resize").boundingBox())!;
  await page.mouse.move(edge.x + edge.width / 2, edge.y + edge.height / 2);
  await page.mouse.down();
  await page.mouse.move(edge.x + edge.width / 2, edge.y - 80, { steps: 5 });
  await page.mouse.up();
  await expect.poll(async () => (await view.boundingBox())!.height).toBeGreaterThan(before + 40);

  // Collapsing hides the view.
  await toggle.click();
  await expect(panel.getByTestId("network-3d")).toHaveCount(0);
});
