import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

/** Builds the line-sensor network with background input; returns the spike count after a 1 s run. */
async function runLineSensor(page: Page, baseURL: string, nodes: string): Promise<number> {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("cluster-nodes").fill(nodes);
  await page.getByTestId("cluster-nodes").blur();
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "line-sensor-layers-wta", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Shards").fill("2");
  await page.getByLabel("Neurons starting with", { exact: true }).fill("input-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("100");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible({ timeout: 20_000 });
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("1");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  const session = Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
  const client = await WebSocketClient.connect(baseURL);
  await expect
    .poll(async () => (await client.listSessions()).find((s) => s.id === session), { timeout: 20_000 })
    .toMatchObject({ state: "ready", simMs: 1000 });
  const network = await client.getNetwork(session);
  let spikes = 0;
  for (const n of network.neurons) spikes += (await client.queryNeuron(session, n.index)).spikes;
  client.close();
  return spikes;
}

test("a network spread over two node agents runs exactly as on the server alone, with the same screens", async ({ page, baseURL }) => {
  test.setTimeout(120_000);
  const alone = await runLineSensor(page, baseURL!, "");
  expect(alone).toBeGreaterThan(100);
  await expect(page.getByTestId("cluster")).toHaveCount(0);

  const spread = await runLineSensor(page, baseURL!, "127.0.0.1:17091, 127.0.0.1:17092");
  expect(spread).toBe(alone); // queried neuron by neuron: most live on the other nodes
  const panel = page.getByTestId("cluster");
  await expect(panel).toContainText("Cluster · 3 nodes");
  await expect(panel.getByTestId("cluster-node")).toHaveCount(3);
  await expect(panel).toContainText("127.0.0.1:17092");
  // Spikes crossed to the nodes (this network sends none back to the coordinator's neurons).
  await expect(panel.getByTestId("cluster-node").nth(2)).toContainText(/[1-9][\d,]+\s*\d/);
  // The usual views show the whole network: spikes from every node reach the raster.
  await expect(page.getByTestId("no-spikes")).toHaveCount(0);

  // Best effort, at real time: each node follows its copy of the clock; the panel shows each link's timeliness.
  await page.getByTestId("lockstep").uncheck();
  await page.getByLabel("As fast as possible").uncheck();
  await page.getByLabel("Run for (s)").fill("1.5");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await expect(page.getByTestId("state")).toHaveText("ready", { timeout: 15_000 });
  await expect(page.getByTestId("cluster-links")).toBeVisible({ timeout: 5000 });
  await expect(page.getByTestId("cluster-link").first()).toContainText("→");
  await expect(page.getByTestId("cluster-link").first()).toContainText("%");
  await page.screenshot({ path: "test-results/cluster.png", fullPage: true });
});
