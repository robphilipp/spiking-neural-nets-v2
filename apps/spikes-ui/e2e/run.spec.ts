import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";

/** An example network (the tests run from apps/spikes-ui). */
const example = (name: string) => join(process.cwd(), "..", "..", "test-data", "networks", name);

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
  await expect(page.getByTestId("network-status")).toHaveText("Applied");

  // Run page: build, stimulate group 1, run at real time.
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("25 neurons")).toBeVisible();
  await page.getByLabel("Stimulate neurons", { exact: true }).fill("hidden-10");
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
  await page.keyboard.type("(neurons=[(name=a, threshold=1 parsec, position=[0, 0, 0])])");
  await page.getByTestId("apply").click();
  await expect(page.locator("[data-sonner-toast]").filter({ hasText: /errors? in the description/ })).toBeVisible();
  await expect(page.locator(".monaco-editor .squiggly-error").first()).toBeAttached();
});

test("the editor suggests keys and explains them", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.locator(".monaco-editor").click();
  await page.keyboard.type("(neurons=[(name=a, dynamics=bistable_integrator, thresh");
  await page.keyboard.press("Control+Space");
  const widget = page.locator(".suggest-widget");
  await expect(widget).toBeVisible();
  // The bistable thresholds, offered because of its dynamics; not the plain threshold.
  await expect(widget).toContainText("tonic_on_threshold");
  await expect(widget.getByRole("option", { name: /^threshold/ })).toHaveCount(0);
  await page.screenshot({ path: "test-results/editor.png" });
  await page.keyboard.press("Escape");
  for (let i = 0; i < "thresh".length; i++) await page.keyboard.press("Backspace");
  // Hover help on a key.
  await page.keyboard.type("decay_time=250 ms, position=[0, 0, 0])])");
  // Monaco shows hover help after a delay; hover again if the first one landed while the editor was settling.
  const help = page.locator(".monaco-hover").filter({ hasText: "decay time constant" });
  await expect(async () => {
    await page.mouse.move(0, 0);
    await page.getByText("decay_time", { exact: true }).hover();
    await expect(help).toBeVisible({ timeout: 1500 });
  }).toPass({ timeout: 10_000 });
  // With the details: the equation, and when it doesn't apply.
  await expect(help).toContainText("u(t + Δ) = u(t) × e^(−Δ/τ)");
  await expect(help).toContainText("for resonators, which use");
  await page.screenshot({ path: "test-results/editor-hover.png" });
  // A longer one, with a list.
  const dynamics = page.locator(".monaco-hover").filter({ hasText: "leaky integrate-and-fire" });
  await expect(async () => {
    await page.mouse.move(0, 0);
    await page.getByText("dynamics", { exact: true }).hover();
    await expect(dynamics).toBeVisible({ timeout: 1500 });
  }).toPass({ timeout: 10_000 });
  await page.screenshot({ path: "test-results/editor-hover-dynamics.png" });
  // An equation's symbols are defined under it.
  await page.keyboard.press("ControlOrMeta+A");
  await page.keyboard.type("(learning_rules=[(name=l, rule=stdp_hard, potentiation=0.01, potentiation_time=20 ms)])");
  const potentiation = page.locator(".monaco-hover").filter({ hasText: "how long before the receiver's new spike it arrived" });
  await expect(async () => {
    await page.mouse.move(0, 0);
    await page.getByText("potentiation", { exact: true }).hover();
    await expect(potentiation).toBeVisible({ timeout: 1500 });
  }).toPass({ timeout: 10_000 });
  await page.screenshot({ path: "test-results/editor-hover-symbols.png" });
});

test("open a .boo file and download it again (browser projects)", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-projects").click();
  await page.getByTestId("file-input").setInputFiles(example("ring.boo"));
  await expect(page.locator(".monaco-editor")).toContainText("hidden-101"); // switched to the editor
  await page.getByTestId("nav-projects").click();
  await expect(page.getByLabel("Project name")).toHaveValue("ring");
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByTestId("download-network").click()]);
  expect(readFileSync(await download.path(), "utf8")).toBe(readFileSync(example("ring.boo"), "utf8"));
});

test("a quiet network explains itself; background input on its inputs brings it to life", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "line-sensor-layers-wta", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();

  // Nothing drives the input layer: no spikes, and the page says why.
  await page.getByTestId("start").click();
  await expect(page.getByTestId("no-spikes")).toBeVisible({ timeout: 5000 });
  await page.getByTestId("stop").click();

  // Background input on the input layer, then build and run again.
  await page.getByTestId("background").check();
  await page.getByLabel("Background neurons", { exact: true }).fill("input-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByLabel("mV", { exact: true }).nth(1).fill("6");
  await page.getByTestId("build").click();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("timing")).toContainText(/Spikes\s*[1-9][\d,]*\/s/, { timeout: 5000 });
  await expect(page.getByTestId("no-spikes")).toBeHidden();
  await page.waitForTimeout(1500);
  await page.screenshot({ path: "test-results/line-sensor.png", fullPage: true });
  await page.getByTestId("stop").click();
});

test("number fields can be emptied and retyped", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();

  await page.getByTestId("background").check();
  const rate = page.getByLabel("Hz per neuron", { exact: true });
  await rate.click();
  await rate.press("ControlOrMeta+a");
  await rate.press("Backspace");
  await expect(rate).toHaveValue(""); // empty, not a minimum like 0.001
  await expect(rate).toHaveAttribute("aria-invalid", "true");
  await rate.pressSequentially("0.5");
  await expect(rate).toHaveValue("0.5");
  await expect(rate).not.toHaveAttribute("aria-invalid");

  // Emptied and left: the last valid value comes back.
  const shards = page.getByLabel("Shards", { exact: true });
  await shards.fill("3");
  await shards.fill("");
  await expect(shards).toHaveValue("");
  await rate.click();
  await expect(shards).toHaveValue("3");
  await expect(rate).toHaveValue("0.5");
});
