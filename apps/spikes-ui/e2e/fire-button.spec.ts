import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";
import { addBackground, addPulse } from "./helpers";

// The Run page's Fire buttons: the input file's pulse sources, beside Edit inputs.

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

  // Another source is added to the file (on the Inputs page): the engine still has the pulse as it was, so Fire stays,
  // while the rebuild is asked for as before.
  await addBackground(page, { neurons: "hidden-3", hz: 5, mv: 12 });
  await expect(page.getByTestId("rebuild-hint")).toBeVisible();
  await expect(fire).toBeEnabled();

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
