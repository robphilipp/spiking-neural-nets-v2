import { expect, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

test("inputs pick neurons by prefix or by regular expression", async ({ page, baseURL }) => {
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

  // Background input: a regular expression, with the count of neurons it matches.
  const background = page.getByTestId("background-neurons");
  await background.getByRole("radio", { name: "Regex" }).click();
  await expect(background.getByRole("radio", { name: "Regex" })).toHaveAttribute("aria-checked", "true");
  await page.getByLabel("Background neurons", { exact: true }).fill("^hidden-[12]0[13]$");
  await expect(page.getByTestId("background-neurons-count")).toHaveText("· 4");
  // An invalid one says so, and matches nothing.
  await page.getByLabel("Background neurons", { exact: true }).fill("hidden-[");
  await expect(background).toContainText("not a regular expression");
  await page.getByLabel("Background neurons", { exact: true }).fill("^hidden-[12]0[13]$");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByTestId("background").check();
  await expect(page.getByTestId("background")).toBeChecked();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await expect(page.getByTestId("background")).toBeChecked();
  await expect(page.getByTestId("rebuild-hint")).toHaveCount(0);
  // The input reaches the four matching neurons, and only them.
  await page.getByTestId("start").click();
  await page.waitForTimeout(1500);
  await page.getByTestId("stop").click();
  const session = Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
  const client = await WebSocketClient.connect(baseURL!);
  const network = await client.getNetwork(session);
  for (const name of ["hidden-101", "hidden-103", "hidden-201", "hidden-203"]) {
    const neuron = network.neurons.find((n) => n.name === name)!;
    expect((await client.queryNeuron(session, neuron.index)).spikes, name).toBeGreaterThan(5);
  }
  client.close();
  await page.screenshot({ path: "test-results/neuron-match.png", clip: { x: 180, y: 140, width: 1220, height: 200 } });

  // Stimulation: a prefix by default; a regular expression on request.
  const stimulus = page.getByTestId("stimulus-neurons");
  await page.getByLabel("Stimulate neurons", { exact: true }).fill("hidden-1");
  await expect(page.getByTestId("stimulus-neurons-count")).toHaveText("· 5");
  await stimulus.getByRole("radio", { name: "Regex" }).click();
  await page.getByLabel("Stimulate neurons", { exact: true }).fill("5$");
  await expect(page.getByTestId("stimulus-neurons-count")).toHaveText("· 5");
  await page.getByTestId("stimulate").click();
  await expect(page.getByText(/no neurons have/)).toHaveCount(0);

  // The Inputs page's targets: one channel per matching neuron.
  await page.getByTestId("nav-inputs").click();
  const target = page.getByTestId("target-neurons");
  await target.getByRole("radio", { name: "Regex" }).click();
  await page.getByLabel("Target neurons", { exact: true }).fill("^hidden-[34]");
  await expect(page.getByText("10 channels, one per neuron")).toBeVisible();
});
