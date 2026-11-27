import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";
import { addPulse } from "./helpers";

// The Run page's quick controls: Stimulate (a one-off pulse, not part of the input file) and the Background input
// switch (the input file's "background" source).

async function openRing(page: Page) {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();
}

const sessionId = async (page: Page) => Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);

/** A neuron's spike count, asked of the engine. */
async function spikes(baseURL: string, session: number, name: string): Promise<number> {
  const client = await WebSocketClient.connect(baseURL);
  const network = await client.getNetwork(session);
  const n = (await client.queryNeuron(session, network.neurons.find((x) => x.name === name)!.index)).spikes;
  client.close();
  return n;
}

test("Stimulate sends a one-off pulse to the neurons it names, before a run and during one", async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await openRing(page);
  const stimulate = page.getByTestId("stimulate");
  await expect(stimulate).toBeDisabled(); // no network built yet
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  const session = await sessionId(page);

  // Neurons that match nothing: an error saying so.
  await page.getByLabel("Stimulate neurons", { exact: true }).fill("zzz");
  await expect(page.getByTestId("stimulus-neurons-count")).toHaveText("· 0");
  await stimulate.click();
  await expect(page.getByText("no neurons have names starting with 'zzz'")).toBeVisible();

  // Before the run: a 15 mV pulse to the first column makes those neurons fire once the run starts.
  await page.getByLabel("Stimulate neurons", { exact: true }).fill("hidden-10");
  await page.getByLabel("mV", { exact: true }).first().fill("15");
  await stimulate.click();
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("0.2");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("ready", { timeout: 20_000 });
  expect(await spikes(baseURL!, session, "hidden-101")).toBeGreaterThan(0);

  // During a run (real time, until stopped): the third column's pulse reaches it.
  await page.getByTestId("reset").click();
  await expect(page.getByTestId("timing")).toContainText(/Simulated time\s*0\.000 s/);
  await page.getByLabel("As fast as possible").uncheck();
  await page.getByLabel("Run for (s)").fill("");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await page.waitForTimeout(400);
  expect(await spikes(baseURL!, session, "hidden-301")).toBe(0); // no input: silent
  await page.getByLabel("Stimulate neurons", { exact: true }).fill("hidden-30");
  await stimulate.click();
  await expect.poll(() => spikes(baseURL!, session, "hidden-301"), { timeout: 10_000 }).toBeGreaterThan(0);
  await page.getByTestId("stop").click();
});

test("the Background input switch edits the input file's background source, and is given to the run at Build", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await openRing(page);
  const background = page.getByTestId("background");

  // Off: its settings are hidden, and the file has no sources.
  await expect(background).not.toBeChecked();
  await expect(page.getByLabel("Background neurons", { exact: true })).toHaveCount(0);
  await expect(page.getByTestId("input-summary")).toHaveText("0 of 0 sources on");

  // On: the settings show, and it's a source of the file.
  await background.check();
  await expect(page.getByTestId("input-summary")).toHaveText("1 of 1 source on");
  await page.getByLabel("Background neurons", { exact: true }).fill("hidden-1");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("30");
  await page.getByLabel("mV", { exact: true }).nth(1).fill("12");
  await page.getByTestId("nav-inputs").click();
  await expect(page.getByTestId("source-row-background")).toContainText("hidden-1"); // the same source, on the Inputs page
  await page.getByTestId("nav-run").click();
  await expect(page.getByLabel("Hz per neuron", { exact: true })).toHaveValue("30");

  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await expect(page.getByTestId("background-neurons-count")).toHaveText("· 5"); // counted once the network is built
  const session = await sessionId(page);
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("1");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("ready", { timeout: 20_000 });
  expect(await spikes(baseURL!, session, "hidden-101")).toBeGreaterThan(5);

  // Changing it asks for a rebuild; while a run is open it can't change.
  await page.getByLabel("Hz per neuron", { exact: true }).fill("40");
  await expect(page.getByTestId("rebuild-hint")).toBeVisible();
  await page.getByTestId("reset").click();
  await page.getByLabel("As fast as possible").uncheck();
  await page.getByLabel("Run for (s)").fill("");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await expect(background).toBeDisabled();
  await expect(page.getByLabel("Hz per neuron", { exact: true })).toBeDisabled();
  await page.waitForTimeout(500); // (the buttons' colours fade as the state changes)
  await page.screenshot({ path: "test-results/run-controls.png", clip: { x: 180, y: 50, width: 1220, height: 510 } });
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");

  // Off again: given to the next build as no source, so nothing fires without other input.
  await expect(background).toBeEnabled();
  await background.uncheck();
  await expect(page.getByTestId("input-summary")).toHaveText("0 of 1 source on");
});

test("a pulse source's Fire button stays usable when other inputs change, and waits for a rebuild when its own pulse does", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await openRing(page);
  await addPulse(page, { name: "tap", neurons: "hidden-10", mv: 15 });
  const fire = page.getByTestId("fire-tap");

  // Beside Edit inputs; not before the network is built, then yes.
  await expect(fire).toBeVisible();
  await expect(page.getByTestId("edit-inputs")).toBeVisible();
  await expect(fire).toBeDisabled();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await expect(fire).toBeEnabled();

  // Another source of the file changes (the background switched on): the engine still has the pulse as it was, so Fire
  // stays, while the rebuild is asked for as before.
  await page.getByTestId("background").check();
  await expect(page.getByTestId("rebuild-hint")).toBeVisible();
  await expect(fire).toBeEnabled();
  await page.getByTestId("background").uncheck();

  // It fires what the engine has: hidden-101 spikes once the run goes.
  const session = await sessionId(page);
  await fire.click();
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("0.2");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("ready", { timeout: 20_000 });
  expect(await spikes(baseURL!, session, "hidden-101")).toBeGreaterThan(0);

  // Its own pulse changed (here its size): the engine would fire the old one, so Fire waits for a rebuild.
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("source-row-tap").click();
  await page.getByLabel("mV", { exact: true }).fill("20");
  await page.getByTestId("nav-run").click();
  await expect(fire).toBeDisabled();
  await expect(fire).toHaveAttribute("title", /Build the network with this input file/);
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await expect(fire).toBeEnabled();
});
