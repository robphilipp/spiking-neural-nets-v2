import { readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

test("the Run page's background input is a source of the project's input file", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();

  // A new project has one input file, selected, empty.
  await expect(page.getByLabel("Input file")).toHaveText("inputs");
  await expect(page.getByTestId("input-summary")).toHaveText("0 of 0 sources on");
  await page.getByLabel("Background neurons", { exact: true }).fill("hidden-1");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByTestId("background").check();
  await expect(page.getByTestId("input-summary")).toHaveText("1 of 1 source on");
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await expect(page.getByTestId("rebuild-hint")).toHaveCount(0);
  // Changing the input file asks for a rebuild.
  await page.getByLabel("Hz per neuron", { exact: true }).fill("30");
  await expect(page.getByTestId("rebuild-hint")).toBeVisible();

  // The file, as the project saves it.
  await page.getByTestId("nav-projects").click();
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Download inputs.inputs.toml" }).click()]);
  const text = readFileSync(await download.path(), "utf8");
  expect(text).toContain('name = "background"');
  expect(text).toContain('kind = "poisson"');
  expect(text).toMatch(/neurons = \{ prefix = "hidden-1" \}/);
  expect(text).toContain("hz = 30");
});

test("the engine runs an input set through the protocol: sets, signals, pulses and fire", async ({ baseURL }) => {
  const client = await WebSocketClient.connect(baseURL!);
  const network = readFileSync(new URL("../../../test-data/examples/nemotode.boo", import.meta.url), "utf8");
  const { id } = await client.createSession("inputs", network);
  await client.build(id, { shards: 2, seed: 3 });
  const net = await client.getNetwork(id);
  const touch = net.sets.find((s) => s.name === "touch_posterior")!;
  expect(touch.members.map((i) => net.neurons[i]!.name).sort()).toEqual(["PLML", "PLMR", "PVM"]);
  const first = net.neurons.find((n) => n.name === "ASHL")!.index;
  const last = net.neurons.find((n) => n.name === "PLML")!.index;

  await client.setInputs(id, [
    {
      name: "wave",
      startMs: 0,
      input: {
        type: "signal",
        channels: [[first], [last]],
        formula: "c == 0 ? 1 : 0",
        sampleMs: 1,
        encoding: { type: "rate", maxHz: 80, mv: 12 },
        durationMs: null,
      },
    },
    { name: "tap", startMs: 0, input: { type: "pulses", targets: [last], atMs: [], mv: 12 } },
  ]);
  // Given once per build.
  await expect(client.setInputs(id, [])).rejects.toThrow(/already has its inputs/);
  await client.fire(id, "tap");
  await client.start(id, { speed: null, durationMs: 1000 });
  await expect.poll(async () => (await client.listSessions()).find((s) => s.id === id)?.simMs, { timeout: 20_000 }).toBe(1000);
  const firstSpikes = (await client.queryNeuron(id, first)).spikes;
  const lastSpikes = (await client.queryNeuron(id, last)).spikes;
  expect(firstSpikes).toBeGreaterThan(20); // ~80 Hz for 1 s, less refractory time
  expect(lastSpikes).toBeLessThan(5); // the one tap (and any input from the network)
  await expect(client.fire(id, "wave")).rejects.toThrow(/isn't a pulse/);
  client.close();
});
