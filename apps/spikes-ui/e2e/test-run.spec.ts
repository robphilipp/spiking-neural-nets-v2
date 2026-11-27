import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

const PULSE_CSV = readFileSync(fileURLToPath(new URL("../../../test-data/projects/ring/pulse.csv", import.meta.url)));
const TRIALS = fileURLToPath(new URL("../../../test-data/projects/ring/trials.inputs.toml", import.meta.url));

async function openExample(page: Page, name: string) {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name, exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
}

const sessionId = async (page: Page) => Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);

/** The number in a results cell of group `name`. */
const cell = (page: Page, name: string, column: "test-spikes" | "test-hz") => page.getByTestId(`test-row-${name}`).getByTestId(column);

test("a test run counts the spikes of the sources' targets and leaves the session alone", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await openExample(page, "ring");
  await page.getByTestId("nav-inputs").click();
  const panel = page.getByTestId("test-panel");

  // Poisson input on the first column, and two taps on the third; no network built.
  await page.getByTestId("add-poisson").click();
  await page.getByLabel("Name", { exact: true }).fill("background");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-1");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByLabel("mV", { exact: true }).fill("10");
  await page.getByTestId("add-pulse").click();
  await page.getByLabel("Name", { exact: true }).fill("tap");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-30");
  await page.getByTestId("pulse-times").fill("500 ms, 1 s");
  await page.getByLabel("mV", { exact: true }).fill("15");
  // Realtime sources are left out, with a note.
  await page.getByTestId("add-sensor").click();
  await page.getByLabel("Name", { exact: true }).fill("probe");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-4");
  await expect(panel.getByTestId("test-realtime-note")).toContainText("probe");

  const client = await WebSocketClient.connect(baseURL!);
  const before = await client.listSessions();
  const id = await sessionId(page);
  expect(before.find((s) => s.id === id)?.state).toBe("loaded");

  // The network isn't built, but the throwaway builds its own: groups per source, and the whole network.
  await panel.getByTestId("test-group-all").click();
  await panel.getByTestId("test-duration").fill("2");
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-results")).toBeVisible({ timeout: 30_000 });
  await expect(panel.getByTestId("test-summary")).toContainText("2.00 s simulated");
  const background = Number(await cell(page, "background", "test-spikes").innerText());
  expect(background).toBeGreaterThan(20); // 5 neurons at 20 Hz for 2 s: about 200 input events
  expect(Number(await cell(page, "background", "test-hz").innerText())).toBeGreaterThan(1);
  // Two taps on five neurons, at 15 mV: each fires at least once.
  expect(Number(await cell(page, "tap", "test-spikes").innerText())).toBeGreaterThanOrEqual(10);
  const all = Number(await cell(page, "all neurons", "test-spikes").innerText());
  expect(all).toBeGreaterThanOrEqual(background);
  expect(Number(await cell(page, "whole network", "test-spikes").innerText())).toBe(all);
  await expect(panel.getByTestId("test-rate-chart").locator("canvas").first()).toBeVisible();
  await expect(panel.getByTestId("test-response-all")).toHaveCount(0); // no trials: no trial-averaged charts
  await panel.screenshot({ path: "test-results/test-run.png" });

  // The same seed gives the same result.
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-run")).toHaveText("Run test");
  expect(await cell(page, "background", "test-spikes").innerText()).toBe(String(background));

  // The real session is as it was: not built, no time. (A test run uses an engine of its own, so it makes no session;
  // but the server is shared with tests running at the same time, which open sessions of their own, so a new session in
  // the list proves nothing: this session's summary being unchanged does.)
  const after = await client.listSessions();
  expect(after.find((s) => s.id === id)).toEqual(before.find((s) => s.id === id));
  await expect(page.getByTestId("test-stale")).toHaveCount(0);

  // Editing the file marks the results stale.
  await page.getByTestId("source-row-background").click();
  await page.getByLabel("Hz per neuron", { exact: true }).fill("30");
  await expect(panel.getByTestId("test-stale")).toBeVisible();
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-stale")).toHaveCount(0);
  expect(Number(await cell(page, "background", "test-spikes").innerText())).toBeGreaterThan(background);

  // A custom group by name, resolved in the network as written.
  await panel.getByTestId("test-custom").getByRole("textbox").fill("hidden-3");
  await panel.getByTestId("test-custom-add").click();
  await expect(panel.getByTestId("test-custom-chip")).toContainText("starts with hidden-3");
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-row-starts with hidden-3")).toBeVisible();

  // Built and running: the machine is busy, so the test is off, and the run isn't disturbed.
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await page.getByTestId("nav-inputs").click();
  await expect(panel.getByTestId("test-run")).toBeDisabled();
  await expect(panel.getByTestId("test-blocked")).toContainText("A run is open");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  const stopped = (await client.listSessions()).find((s) => s.id === id)!;

  // Built: sets, groups and the test run leave it as it is (its time doesn't move).
  await page.getByTestId("nav-inputs").click();
  await expect(panel.getByTestId("test-run")).toBeEnabled();
  // (The panel keeps its settings and result across pages.)
  await expect(panel.getByTestId("test-duration")).toHaveValue("2");
  await expect(panel.getByTestId("test-summary")).toContainText("2.00 s simulated");
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-summary")).toContainText("2.00 s simulated");
  const tested = (await client.listSessions()).find((s) => s.id === id)!;
  expect(tested).toEqual(stopped);
  expect(stopped.simMs).toBeGreaterThan(0);
  client.close();
});

test("a test run with trials marks them and averages each condition", async ({ page }) => {
  test.setTimeout(90_000);
  await openExample(page, "ring");
  await page.getByTestId("nav-projects").click();
  await page.getByTestId("inputs-file-input").setInputFiles(TRIALS);
  await expect(page.getByText("Opened trials.inputs.toml")).toBeVisible();
  await page.getByTestId("nav-inputs").click();
  const panel = page.getByTestId("test-panel");

  // The duration defaults to the end of the trial schedule.
  const duration = Number(await panel.getByTestId("test-duration").inputValue());
  expect(duration).toBeGreaterThan(5);
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-results")).toBeVisible({ timeout: 45_000 });
  await expect(panel.getByTestId("test-summary")).toContainText(`${duration.toFixed(2)} s simulated`);
  // Bands for the 8 trials on the rate chart, and a response chart per condition.
  await expect(panel.getByTestId("test-rate-chart-band")).toHaveCount(8);
  for (const condition of ["column 1", "column 3"]) {
    await expect(panel.getByTestId(`test-response-${condition}`)).toContainText("4 trials");
    await expect(panel.getByTestId(`test-response-chart-${condition}`).locator("canvas").first()).toBeVisible();
  }
  // The tapped column spikes during the trial: the sources' groups have spikes.
  expect(Number(await cell(page, "tap 1", "test-spikes").innerText())).toBeGreaterThan(0);
  expect(Number(await cell(page, "tap 3", "test-spikes").innerText())).toBeGreaterThan(0);
  await expect(panel.getByTestId("test-incomplete")).toHaveCount(0);
  await panel.screenshot({ path: "test-results/test-run-trials.png" });
});

test("a test run plays a data file: the source's targets spike in the data's pattern", async ({ page }) => {
  test.setTimeout(90_000);
  await openExample(page, "ring");
  await page.getByTestId("nav-inputs").click();
  const panel = page.getByTestId("test-panel");
  // A CSV opened in the browser, played by a file source into the first column's neurons (three channels, each on for
  // 200 ms in turn: the source's data goes to the throwaway with the test).
  await page.getByTestId("data-file-input").setInputFiles({ name: "pulse.csv", mimeType: "text/csv", buffer: PULSE_CSV });
  await expect(page.getByTestId("data-info-pulse")).toContainText("3 channels");
  await page.getByTestId("add-file").click();
  const editor = page.getByTestId("source-editor");
  await page.getByLabel("Name", { exact: true }).fill("odour");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-10");
  await page.getByTestId("file-data").click();
  await page.getByRole("option", { name: "pulse", exact: true }).click();
  await editor.getByLabel("Hz at 1").fill("300");
  await editor.getByLabel("mV per event").fill("15");

  // 0.5 s: only the first channel has played; 1.5 s: all three have.
  await panel.getByTestId("test-duration").fill("0.5");
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-summary")).toContainText("0.50 s simulated", { timeout: 30_000 });
  const early = Number(await cell(page, "odour", "test-spikes").innerText());
  expect(early).toBeGreaterThan(3);
  await panel.getByTestId("test-duration").fill("1.5");
  await panel.getByTestId("test-run").click();
  await expect(panel.getByTestId("test-summary")).toContainText("1.50 s simulated", { timeout: 30_000 });
  const later = Number(await cell(page, "odour", "test-spikes").innerText());
  expect(later).toBeGreaterThan(2 * early); // three channels played, not one
});
