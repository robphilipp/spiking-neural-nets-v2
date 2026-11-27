import { expect, test } from "@playwright/test";
import { addBackground } from "./helpers";

test("the 3D view draws the network, flashes spikes, and selects a region for the raster", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await addBackground(page, { neurons: "hidden-1", hz: 20, mv: 10 });
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

test("hovering a neuron highlights it and shows its name, position and state", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();

  // The pointer over the first neuron.
  const view = page.getByTestId("network-3d");
  const b = (await view.boundingBox())!;
  const tooltip = page.getByTestId("neuron-tooltip");
  const at = (await page.evaluate(
    () => (window as unknown as { __spikes3d: { project: (i: number) => { x: number; y: number } } }).__spikes3d.project(0),
  ))!;
  await page.mouse.move(b.x + at.x, b.y + at.y);
  await expect(tooltip).toBeVisible();
  await expect(tooltip).toContainText(/hidden-10\d/); // (or a neighbour in front of it)
  await expect(tooltip).toContainText("µm");
  await expect(tooltip).toContainText(/excitatory|inhibitory/);
  await expect(page.getByTestId("neuron-tooltip-state")).toContainText(/Membrane\s*-?\d+\.\d\d mV/);
  await expect(page.getByTestId("neuron-tooltip-state")).toContainText(/Spikes\s*\d+/);
  await page.screenshot({ path: "test-results/neuron-tooltip.png" });

  // Away from the view: gone.
  await page.mouse.move(b.x + b.width / 2, b.y + b.height + 200);
  await expect(tooltip).toBeHidden();

  // Tooltips off (on by default): hovering shows nothing, until they're on again.
  const toggle = page.getByTestId("show-tooltips");
  await expect(toggle).toBeChecked();
  await toggle.click();
  await expect(toggle).not.toBeChecked();
  await page.mouse.move(b.x + at.x, b.y + at.y);
  await page.waitForTimeout(300);
  await expect(tooltip).toBeHidden();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height + 200);
  await toggle.click();
  await page.mouse.move(b.x + at.x, b.y + at.y);
  await expect(tooltip).toBeVisible();
});

test("coming back to the Run page, the 3D view is drawn at its full width from the first frame", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();
  await expect(page.getByTestId("network-3d").locator("canvas")).toBeVisible();

  for (let i = 0; i < 3; i++) {
    await page.getByTestId("nav-network").click();
    // The canvas's width in the first frame the Run page paints.
    const widths = await page.evaluate(
      () =>
        new Promise<[number, number]>((resolve) => {
          (document.querySelector('[data-testid="nav-run"]') as HTMLElement).click();
          const look = () => {
            const view = document.querySelector('[data-testid="network-3d"]');
            const canvas = view?.querySelector("canvas");
            if (!view || !canvas) return requestAnimationFrame(look);
            resolve([canvas.getBoundingClientRect().width, document.querySelector('[data-testid="raster"]')!.getBoundingClientRect().width]);
          };
          requestAnimationFrame(look);
        }),
    );
    expect(Math.abs(widths[0] - widths[1])).toBeLessThan(2);
  }
});

test("the 3D view keeps its rotation, position and zoom when leaving the page and coming back", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();
  const view = page.getByTestId("network-3d");
  await expect(view.locator("canvas")).toBeVisible();

  const where = () =>
    page.evaluate(() => {
      const s = (window as unknown as { __spikes3d: { project: (i: number) => { x: number; y: number } } }).__spikes3d;
      return [s.project(0), s.project(12), s.project(24)];
    });
  const fresh = await where();

  // Scrolling alone leaves the view as it is (it scrolls the page); shift-scroll zooms: neurons spread apart.
  const b = (await view.boundingBox())!;
  const spread = (p: Array<{ x: number; y: number }>) => Math.hypot(p[0]!.x - p[2]!.x, p[0]!.y - p[2]!.y);
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.wheel(0, -300);
  await page.waitForTimeout(200);
  expect(await where()).toEqual(fresh);
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, -300);
  await page.keyboard.up("Shift");
  await page.waitForTimeout(200);
  const zoomed = spread(await where());
  expect(zoomed).toBeGreaterThan(spread(fresh) * 1.2);
  // Ctrl-scroll (and a trackpad pinch, which arrives as one) zooms too: back out.
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, 200);
  await page.keyboard.up("Control");
  await page.waitForTimeout(200);
  expect(spread(await where())).toBeLessThan(zoomed * 0.8);
  await expect(view.locator("..").getByText(/shift- or ctrl-scroll \(or pinch\) to zoom/)).toBeVisible();

  // Orbit (drag), pan (right-drag) and zoom (shift-scroll).
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2 + 120, b.y + b.height / 2 + 40, { steps: 8 });
  await page.mouse.up();
  await page.mouse.down({ button: "right" });
  await page.mouse.move(b.x + b.width / 2 + 60, b.y + b.height / 2 + 80, { steps: 8 });
  await page.mouse.up({ button: "right" });
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, -300);
  await page.keyboard.up("Shift");
  await page.waitForTimeout(300);
  const moved = await where();
  expect(moved).not.toEqual(fresh);

  // Away and back: just as it was left.
  await page.getByTestId("nav-network").click();
  await expect(view).toHaveCount(0);
  await page.getByTestId("nav-run").click();
  await expect(view.locator("canvas")).toBeVisible();
  const back = await where();
  for (let i = 0; i < back.length; i++) {
    expect(back[i]!.x).toBeCloseTo(moved[i]!.x, 0);
    expect(back[i]!.y).toBeCloseTo(moved[i]!.y, 0);
  }
});
