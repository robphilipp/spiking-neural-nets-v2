import { expect, type Page, test } from "@playwright/test";

/** The weights in the inspector's synapse table, by sender. */
async function weights(page: Page): Promise<Record<string, number>> {
  const rows = await page.getByTestId("synapses").locator("tbody tr").all();
  const result: Record<string, number> = {};
  for (const row of rows) {
    const cells = await row.locator("td").allInnerTexts();
    result[cells[0]!] = Number(cells[2]);
  }
  return result;
}

test("reward learning, seen end to end: dopamine turns marked synapses into learning", async ({ page }) => {
  test.setTimeout(60_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "reward", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");

  await page.getByTestId("nav-run").click();
  await page.getByLabel("Neurons starting with", { exact: true }).fill("in-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByLabel("mV", { exact: true }).nth(1).fill("6");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  await expect(page.getByText("3 neurons")).toBeVisible();

  // Inspect the output neuron.
  await page.getByLabel(/Neurons \(up to/).fill("out");
  await page.getByRole("option", { name: "out", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("synapses").locator("tbody tr")).toHaveCount(2);

  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await page.waitForTimeout(1500);
  // Pairings mark the synapses (eligibility), but without dopamine the weights don't move.
  expect(await weights(page)).toEqual({ "in-1": 1, "in-2": 1 });
  const eligibility = await page.getByTestId("synapses").locator("tbody tr td:nth-child(4)").allInnerTexts();
  expect(eligibility.some((e) => Number(e) > 0)).toBe(true);

  // Reward.
  await page.getByTestId("release-dopamine").click();
  await expect.poll(async () => Object.values(await weights(page)).every((w) => w > 1), { timeout: 5000 }).toBe(true);
  await expect(page.getByTestId("inspector").locator("canvas")).toHaveCount(2); // membrane and weight charts
  await page.waitForTimeout(800);
  await page.getByTestId("inspector").screenshot({ path: "test-results/inspector.png" });

  // Prune a connection and add it back, while running.
  await page.getByTestId("synapses").locator("tbody tr").first().getByTitle("Prune this connection").click();
  await expect(page.getByTestId("synapses").locator("tbody tr")).toHaveCount(1);
  await page.getByLabel("Sender").fill("in-1");
  await page.getByRole("option", { name: "in-1", exact: true }).click();
  await page.getByTestId("connect").click();
  await expect(page.getByTestId("synapses").locator("tbody tr")).toHaveCount(2);
  await page.getByTestId("stop").click();
});

test("background input changed after building asks for a rebuild; the dopamine button is near the controls", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "reward", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("3 neurons")).toBeVisible();

  // The release button is visible without scrolling.
  await expect(page.getByTestId("release-dopamine")).toBeInViewport();

  // Switching background input on after building: Build turns into "Rebuild to apply", with a hint.
  await page.getByLabel("Neurons starting with", { exact: true }).fill("in-");
  await page.getByTestId("background").check();
  await expect(page.getByTestId("build")).toHaveText(/Rebuild to apply/);
  await expect(page.getByTestId("rebuild-hint")).toBeVisible();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("build")).toHaveText(/^Build$/);
  await expect(page.getByTestId("rebuild-hint")).toHaveCount(0);
  await page.getByTestId("start").click();
  await expect(page.getByTestId("timing")).toContainText(/Spikes\s*[1-9][\d,]*\/s/, { timeout: 5000 });
  await page.screenshot({ path: "test-results/run-controls.png" });
  await page.getByTestId("stop").click();
});
