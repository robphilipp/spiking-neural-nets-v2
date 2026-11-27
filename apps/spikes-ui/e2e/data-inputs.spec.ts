import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

// Data files, file sources and position mapping of channels (app plan 05, phase I5b), on the line-sensor example: 49
// input neurons named input-<col><row> (1…7 each) on a 7×7 lattice in the xy plane, with no incoming connections, so
// what an input neuron does is what its source sent.

const PULSE_CSV = readFileSync(fileURLToPath(new URL("../../../test-data/projects/ring/pulse.csv", import.meta.url)));

async function openLineSensor(page: Page) {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "line-sensor-layers-wta", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
}

async function sessionId(page: Page): Promise<number> {
  return Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
}

/** Builds on the Run page and runs `seconds` of simulated time as fast as possible; the session. */
async function runFor(page: Page, baseURL: string, seconds: number): Promise<number> {
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  const session = await sessionId(page);
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill(String(seconds));
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  const client = await WebSocketClient.connect(baseURL);
  await expect
    .poll(async () => (await client.listSessions()).find((s) => s.id === session), { timeout: 20_000 })
    .toMatchObject({ state: "ready", simMs: seconds * 1000 });
  client.close();
  return session;
}

/** Spikes per input neuron, by name, asked of the engine. */
async function inputSpikes(baseURL: string, session: number): Promise<Record<string, number>> {
  const client = await WebSocketClient.connect(baseURL);
  const network = await client.getNetwork(session);
  const out: Record<string, number> = {};
  for (const n of network.neurons.filter((x) => x.name.startsWith("input-"))) out[n.name] = (await client.queryNeuron(session, n.index)).spikes;
  client.close();
  return out;
}

/** Picks an option of a select (and waits for it to close: it gives the focus back to its button as it does). */
async function choose(page: Page, testId: string, option: string) {
  await page.getByTestId(testId).click();
  await page.getByRole("option", { name: option, exact: true }).click();
  await expect(page.getByRole("option", { name: option, exact: true })).toHaveCount(0);
  await expect(page.getByTestId(testId)).toBeFocused();
}

/** Orbits the targets view (drag) so the sheet of input neurons faces the camera, and screenshots it. */
async function shootTargets(page: Page, path: string) {
  const view = page.getByTestId("targets-view");
  const box = (await view.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + 100);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 160, box.y + 100, { steps: 10 });
  await page.mouse.up();
  await view.screenshot({ path });
}

const only = (spikes: Record<string, number>, name: string) => Object.entries(spikes).filter(([n, s]) => s > 0 && n !== name);

test("a CSV opened in the browser is played by a file source: the targets spike in the data's pattern", async ({ page, baseURL }) => {
  test.setTimeout(120_000);
  await openLineSensor(page);
  // (The built network says how many neurons the source drives and its channels.)
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("data-file-input").setInputFiles({ name: "pulse.csv", mimeType: "text/csv", buffer: PULSE_CSV });
  await expect(page.getByTestId("data-info-pulse")).toContainText("3 channels · 1.201 s");

  await page.getByTestId("add-file").click();
  const editor = page.getByTestId("source-editor");
  await page.getByLabel("Name", { exact: true }).fill("odour");
  await page.getByLabel("Target neurons", { exact: true }).fill("input-1");
  // Not chosen yet: says so, in place.
  await expect(page.getByTestId("file-problem")).toContainText("no data file called ''");
  await choose(page, "file-data", "pulse");
  await expect(page.getByTestId("file-info")).toContainText("pulse: 3 channels · 1.201 s");
  await editor.getByLabel("Hz at 1").fill("300");
  await editor.getByLabel("mV per event").fill("15");
  await expect(page.getByTestId("source-channels-odour")).toHaveText("3 channels");
  await expect(page.getByTestId("preview").locator("canvas")).toBeVisible();

  // 0.5 s: channel a (0.2–0.4 s) has played, the others haven't started.
  let spikes = await inputSpikes(baseURL!, await runFor(page, baseURL!, 0.5));
  expect(spikes["input-11"]).toBeGreaterThan(3); // 200 ms at up to 300 Hz
  expect(spikes["input-12"]).toBe(0);
  expect(spikes["input-13"]).toBe(0);
  // 1.5 s: all three channels have played, each into its neuron; the fourth target gets nothing.
  spikes = await inputSpikes(baseURL!, await runFor(page, baseURL!, 1.5));
  for (const n of ["input-11", "input-12", "input-13"]) expect(spikes[n], n).toBeGreaterThan(3);
  expect(spikes["input-14"]).toBe(0);
  expect(spikes["input-21"]).toBe(0);
});

test("a data file that can't be read says why; one in use can be removed", async ({ page }) => {
  await openLineSensor(page);
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("data-file-input").setInputFiles({ name: "bad.csv", mimeType: "text/csv", buffer: Buffer.from("t,a\n0,1\n0,2\n") });
  await expect(page.getByTestId("inputs-page").getByText("line 3: times must be ascending")).toBeVisible();
  await expect(page.getByTestId("data-panel")).toContainText("No data files.");
  await page.getByTestId("data-file-input").setInputFiles({ name: "pulse.csv", mimeType: "text/csv", buffer: PULSE_CSV });
  await expect(page.getByTestId("data-row-pulse")).toBeVisible();
  await page.getByTestId("data-remove-pulse").click();
  await expect(page.getByTestId("data-row-pulse")).toHaveCount(0);
});

test("positions map channels onto a sheet of neurons, coloured by channel in 3D", async ({ page, baseURL }) => {
  test.setTimeout(150_000);
  await openLineSensor(page);
  // The network must be built for the Inputs page to colour its targets.
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-signal").click();
  const editor = page.getByTestId("source-editor");
  await page.getByLabel("Target neurons", { exact: true }).fill("input-");
  await editor.getByLabel("Hz at 1").fill("300");
  await editor.getByLabel("mV per event").fill("15");
  await page.getByTestId("formula").fill("c == 24"); // channel 24 of a 7x7 grid: column 3, row 3 (from 0)
  await expect(page.getByTestId("channel-legend")).toHaveCount(0); // the default order isn't coloured

  // By name: input-<column><row>, counted from 1, in a 7x7 grid.
  await choose(page, "layout", "by name");
  await editor.getByLabel("Pattern (regular expression)").fill("^input-(\\d)(\\d)$");
  await editor.getByLabel("Names count from").fill("1");
  await editor.getByLabel("Grid (columns×rows)").fill("7x7");
  await expect(page.getByTestId("channel-plan")).toContainText("49 channels for 49 target neurons");
  await expect(page.getByTestId("layout-warning")).toHaveCount(0);
  await expect(page.getByTestId("channel-legend")).toBeVisible();
  await shootTargets(page, "test-results/layout-name.png");
  let spikes = await inputSpikes(baseURL!, await runFor(page, baseURL!, 0.5));
  expect(spikes["input-44"]).toBeGreaterThan(20);
  expect(only(spikes, "input-44")).toEqual([]);

  // By position: the same cell of the same lattice.
  await page.getByTestId("nav-inputs").click();
  await choose(page, "layout", "grid (by position)");
  await expect(page.getByTestId("layout-plane")).toBeVisible();
  await expect(page.getByTestId("channel-plan")).toContainText("49 channels for 49 target neurons");
  await expect(page.getByTestId("channel-legend")).toBeVisible();
  await shootTargets(page, "test-results/layout-grid.png");
  spikes = await inputSpikes(baseURL!, await runFor(page, baseURL!, 0.5));
  expect(spikes["input-44"]).toBeGreaterThan(20);
  expect(only(spikes, "input-44")).toEqual([]);

  // A finer grid than the lattice leaves cells empty: reported in place.
  await page.getByTestId("nav-inputs").click();
  await editor.getByLabel("Grid (columns×rows)").fill("8x8");
  await expect(page.getByTestId("layout-warning")).toContainText(/\d+ of 64 channels get no neuron/);
  await expect(page.getByTestId("source-mapping-signal")).toBeVisible();

  // Slabs along x: seven columns of seven neurons; channel 24 of 49 is the slab of the 25th (in the middle).
  await choose(page, "layout", "axis (slabs by position)");
  await editor.getByLabel("Channels", { exact: true }).fill("7");
  await page.getByTestId("formula").fill("c == 3");
  await expect(page.getByTestId("channel-plan")).toContainText("7 channels for 49 target neurons");
  spikes = await inputSpikes(baseURL!, await runFor(page, baseURL!, 0.5));
  const driven = Object.entries(spikes).filter(([, s]) => s > 0).map(([n]) => n).sort();
  expect(driven).toEqual(["input-41", "input-42", "input-43", "input-44", "input-45", "input-46", "input-47"]);

  // A pattern that isn't a regular expression is an error in place.
  await page.getByTestId("nav-inputs").click();
  await choose(page, "layout", "by name");
  await editor.getByLabel("Pattern (regular expression)").fill("((");
  await expect(editor.getByText(/not a valid regular expression/)).toBeVisible();
});
