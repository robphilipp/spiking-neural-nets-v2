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

  // A Poisson source on the Inputs page: a regular expression, with the count of neurons it matches.
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-poisson").click();
  const target = page.getByTestId("target-neurons");
  await target.getByRole("radio", { name: "Regex" }).click();
  await expect(target.getByRole("radio", { name: "Regex" })).toHaveAttribute("aria-checked", "true");
  await page.getByLabel("Target neurons", { exact: true }).fill("^hidden-[12]0[13]$");
  await expect(page.getByTestId("target-neurons-count")).toHaveText("· 4");
  await expect(page.getByTestId("source-count-poisson")).toContainText("4 neurons"); // and in the source list
  // An invalid one says so, and matches nothing.
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-[");
  await expect(target).toContainText("not a regular expression");
  await page.getByLabel("Target neurons", { exact: true }).fill("^hidden-[12]0[13]$");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
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

  // A signal source's channels: one per matching neuron.
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-signal").click();
  await page.getByTestId("target-neurons").getByRole("radio", { name: "Regex" }).click();
  await page.getByLabel("Target neurons", { exact: true }).fill("^hidden-[34]");
  await expect(page.getByTestId("preview")).toContainText("first 8 of 10 channels");
});
