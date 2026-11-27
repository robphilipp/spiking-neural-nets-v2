import { expect, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

// The settings given with the wta-learning example: they train out-1 to follow in-1 and out-2 in-2.
test("the winner-takes-all example learns from a signal source", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "wta-learning", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");

  // Inputs page: a signal source: one input at a time, chosen pseudo-randomly every 300 ms, as Poisson spikes at 100 Hz,
  // for the whole 60 s run (it's in the input file, so Build gives it to the run).
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-signal").click();
  const editor = page.getByTestId("source-editor");
  await page.getByLabel("Target neurons", { exact: true }).fill("in-");
  await editor.getByLabel("Encoding").click();
  await page.getByRole("option", { name: "rate (Poisson)" }).click();
  await editor.getByLabel("Hz at 1").fill("100");
  await editor.getByLabel("mV per event").fill("10");
  await page.getByTestId("formula").fill("floor(abs(sin(floor(t/300) * 12.9898) * 43758.5453)) % n == c");
  await page.getByLabel("Duration").fill("60 s");
  await expect(page.getByTestId("preview").locator("canvas")).toBeVisible();

  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("4 neurons")).toBeVisible();
  const session = Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);

  // Run page: as fast as possible, for 60 s.
  await page.getByTestId("nav-run").click();
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("60");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  const client = await WebSocketClient.connect(baseURL!);
  await expect
    .poll(async () => (await client.listSessions()).find((s) => s.id === session), { timeout: 30_000 })
    .toMatchObject({ state: "ready", simMs: 60_000 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: "test-results/wta-run.png", fullPage: true });

  // out-1 (index 2) follows in-1 (0); out-2 (3) follows in-2 (1).
  const weights = async (post: number) =>
    Object.fromEntries((await client.queryNeuron(session, post)).synapses.map((s) => [s.pre, s.weight]));
  const [w1, w2] = [await weights(2), await weights(3)];
  client.close();
  expect(w1[0]).toBeGreaterThan(2.5);
  expect(w1[1]).toBeLessThan(0.3);
  expect(w2[1]).toBeGreaterThan(2.5);
  expect(w2[0]).toBeLessThan(0.3);

  // The inspector shows it too.
  await page.getByTestId("inspect-picker").getByRole("combobox").fill("out-1");
  await page.getByRole("option", { name: "out-1", exact: true }).click();
  await expect(page.getByTestId("synapses")).toContainText("3.0000");
  await page.getByTestId("inspector").screenshot({ path: "test-results/wta-learned.png" });
});
