import { expect, type Page, test } from "@playwright/test";

async function open(page: Page, example: string, nodes: string) {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  if (nodes) {
    await page.getByTestId("nav-settings").click();
    await page.getByTestId("cluster-nodes").fill(nodes);
    await page.getByTestId("cluster-nodes").blur();
  }
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: example, exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByText("Applied.")).toBeVisible();
}

test("the Deployment page shows the hosts, how populations spread over shards, and colours the view", async ({ page }) => {
  await open(page, "populations", "");
  await page.getByTestId("nav-deployment").click();
  await expect(page.getByText("Build the network")).toBeVisible();

  await page.getByTestId("nav-run").click();
  await page.getByLabel("Shards").fill("3");
  await page.getByTestId("build").click();
  await expect(page.getByText("4,000 neurons")).toBeVisible({ timeout: 20_000 });
  await page.getByTestId("nav-deployment").click();

  const hosts = page.getByTestId("deployment-hosts");
  await expect(hosts.locator("tbody tr")).toHaveCount(1);
  await expect(hosts).toContainText("main");
  await expect(hosts).toContainText("this engine");
  await expect(hosts).toContainText("4,000 (100%)");
  await expect(page.getByText("3 per node")).toBeVisible();
  const groups = page.getByTestId("deployment-groups");
  await expect(groups.locator("tbody tr")).toHaveCount(2);
  await expect(groups.locator("tbody tr").first()).toContainText("exc");
  await expect(groups.locator("tbody tr").first()).toContainText("auto");
  await expect(groups.locator("tbody tr").first()).toContainText("3,200 (100%)");

  // Coloured by shard, then a population highlighted.
  const view = page.getByTestId("network-3d");
  await expect(view.locator("canvas")).toBeVisible();
  await page.getByTestId("deployment-color-by").click();
  await page.getByRole("option", { name: "Shard" }).click();
  await groups.locator("tbody tr").first().click();
  await expect(groups.locator("tbody tr").first()).toHaveAttribute("data-state", "selected");
  await page.waitForTimeout(300);
  await page.getByTestId("deployment").screenshot({ path: "test-results/deployment.png" });
  await expect(page.getByTestId("deployment-links")).toHaveCount(0);
});

test("on a cluster, the Deployment page lists every node, where each population runs, and the traffic", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page, "pinned-nodes", "127.0.0.1:17091, edge=127.0.0.1:17092");
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Neurons starting with", { exact: true }).fill("sensors-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("50");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  await expect(page.getByText("220 neurons")).toBeVisible({ timeout: 20_000 });
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("0.5");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("timing")).toContainText(/Simulated time0\.5/, { timeout: 20_000 });
  await expect(page.getByTestId("state")).toHaveText("ready");

  await page.getByTestId("nav-deployment").click();
  const hosts = page.getByTestId("deployment-hosts");
  await expect(hosts.locator("tbody tr")).toHaveCount(3);
  await expect(hosts.locator("tbody tr").nth(1)).toContainText("node 1");
  await expect(hosts.locator("tbody tr").nth(2)).toContainText("edge");
  await expect(hosts.locator("tbody tr").nth(2)).toContainText("127.0.0.1:17092");
  // The sensors are pinned to edge: all 20 there.
  const sensors = page.getByTestId("deployment-groups").locator("tbody tr").first();
  await expect(sensors).toContainText("sensors");
  await expect(sensors.locator("td").nth(1)).toHaveText("edge");
  await expect(sensors.locator("td").nth(4)).toHaveText("20 (100%)");
  await expect(page.getByTestId("deployment-warning")).toHaveCount(0);
  // Spikes crossed from edge to the others.
  const links = page.getByTestId("deployment-links");
  await expect(links.locator("tbody tr").nth(2).locator("td").nth(1)).toHaveText(/^[1-9]/);
  await page.getByTestId("deployment").screenshot({ path: "test-results/deployment-cluster.png" });
});
