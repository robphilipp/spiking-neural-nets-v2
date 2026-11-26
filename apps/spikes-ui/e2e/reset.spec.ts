import { expect, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

test("Reset puts the network back as built, and the next run starts from time 0", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "wta-learning", exact: true }).click();
  await page.getByTestId("apply").click();
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Neurons starting with", { exact: true }).fill("in-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("50");
  await page.getByTestId("background").check();
  await expect(page.getByTestId("reset")).toBeDisabled();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  const session = Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
  const client = await WebSocketClient.connect(baseURL!);
  const weights = async () => (await client.queryNeuron(session, 2)).synapses.map((s) => s.weight);
  const built = await weights();

  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("20");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect.poll(async () => (await client.listSessions()).find((s) => s.id === session), { timeout: 30_000 }).toMatchObject({ state: "ready", simMs: 20_000 });
  expect(await weights()).not.toEqual(built); // it learned

  await page.getByTestId("reset").click();
  await expect(page.getByTestId("timing")).toContainText(/Simulated time\s*0\.000 s/);
  expect(await weights()).toEqual(built);
  expect((await client.listSessions()).find((s) => s.id === session)).toMatchObject({ state: "ready", simMs: 0 });

  // The next run starts from 0, background input and all; the charts start over with it.
  await page.getByLabel("Run for (s)").fill("2");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect.poll(async () => (await client.listSessions()).find((s) => s.id === session), { timeout: 30_000 }).toMatchObject({ state: "ready", simMs: 2000 });
  expect((await client.queryNeuron(session, 0)).spikes).toBeGreaterThan(10);
  await page.waitForTimeout(1000);
  await page.getByTestId("raster").screenshot({ path: "test-results/reset-raster.png" });
  client.close();
});
