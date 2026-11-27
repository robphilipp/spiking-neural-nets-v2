import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

/** The line-sensor example, applied and (unless `build` is false) built. */
async function openLineSensor(page: Page, build = true) {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "line-sensor-layers-wta", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  if (!build) return;
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();
}

/** The session the page works with, from the session picker ("… · session N"). */
async function sessionId(page: Page): Promise<number> {
  return Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
}

/** Total spikes over all neurons, asked of the engine directly. */
async function totalSpikes(baseURL: string, session: number): Promise<number> {
  const client = await WebSocketClient.connect(baseURL);
  const network = await client.getNetwork(session);
  let total = 0;
  for (const n of network.neurons) total += (await client.queryNeuron(session, n.index)).spikes;
  client.close();
  return total;
}

test("a signal source drives the network, the same way every time", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await openLineSensor(page, false);
  // A signal source on the Inputs page: one of 7 stripes at a time, as Poisson events (it's in the input file).
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-signal").click();
  const editor = page.getByTestId("source-editor");
  await page.getByLabel("Target neurons", { exact: true }).fill("input-");
  await editor.getByLabel("Hz at 1").fill("200");
  await editor.getByLabel("mV per event").fill("6");
  await page.getByTestId("formula").fill("floor(t / 100) % 7 == c % 7");
  await page.getByLabel("Duration").fill("2 s");
  await expect(page.getByTestId("preview").locator("canvas")).toBeVisible();
  await page.getByTestId("nav-run").click();
  const counts: number[] = [];
  let session = 0;
  for (let round = 0; round < 2; round++) {
    await page.getByTestId("build").click();
    await expect(page.getByTestId("state")).toHaveText("ready");
    session = await sessionId(page);
    await page.getByLabel("As fast as possible").check();
    await page.getByLabel("Run for (s)").fill("2");
    await page.getByLabel("Run for (s)").blur();
    await page.getByTestId("start").click();
    // Wait for the run to finish at exactly 2 s (the state chip reads "ready" before the run starts, too).
    const client = await WebSocketClient.connect(baseURL!);
    await expect
      .poll(async () => (await client.listSessions()).find((s) => s.id === session), { timeout: 10_000 })
      .toMatchObject({ state: "ready", simMs: 2000 });
    client.close();
    counts.push(await totalSpikes(baseURL!, session));
  }
  expect(counts[0]).toBeGreaterThan(100);
  expect(counts[1]).toBe(counts[0]); // reproducible
});

test("a formula with a mistake says where", async ({ page }) => {
  await openLineSensor(page);
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-signal").click();
  await page.getByTestId("formula").fill("sin(t +");
  await expect(page.getByTestId("source-editor").getByText(/at character/)).toBeVisible();
  await expect(page.getByTestId("preview")).toHaveCount(0);
});

/** Whether this machine is processing audio at all (it may not be: idle, or no output device): an audio clock that
 *  advances in real time. */
async function audioRuns(page: Page): Promise<boolean> {
  return page.evaluate(async () => {
    const ctx = new AudioContext();
    const t0 = ctx.currentTime;
    await new Promise((r) => setTimeout(r, 300));
    const advanced = ctx.currentTime - t0;
    await ctx.close();
    return advanced > 0.15;
  });
}

test("live streaming and the microphone drive a running network", async ({ page }) => {
  test.setTimeout(60_000);
  await openLineSensor(page);
  const sources = (await audioRuns(page)) ? ["stream", "microphone"] : ["stream"];
  if (sources.length === 1) {
    test.info().annotations.push({ type: "skipped", description: "the microphone: this machine isn't processing audio right now" });
  }
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  for (const source of sources) {
    await page.getByTestId("nav-inputs").click();
    if (source === "microphone") await page.getByTestId("realtime-panel").getByLabel(/Hz at 1/).fill("400");
    await page.getByTestId(source).click();
    await expect(page.getByTestId("stop-input")).toBeVisible();
    // It keeps streaming while the Run page shows it.
    await page.getByTestId("nav-run").click();
    await expect(page.getByTestId("timing")).toContainText(/Spikes\s*[1-9][\d,]*\/s/, { timeout: 5000 });
    await page.getByTestId("stop-input-run").click();
    await expect(page.getByTestId("no-spikes")).toBeVisible({ timeout: 5000 }); // quiet again once stopped
  }
  await page.getByTestId("stop").click();
});
