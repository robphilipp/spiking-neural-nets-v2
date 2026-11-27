import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

async function openRing(page: Page) {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
}

test("input files: new, rename, duplicate, delete; the selected one is the Run page's", async ({ page }) => {
  await openRing(page);
  await page.getByTestId("nav-inputs").click();
  const selector = page.getByLabel("Input file (used by runs)");
  await expect(selector).toHaveText("inputs");

  await page.getByTestId("file-new").click();
  await expect(selector).toHaveText("inputs-2");
  // Renamed when you press Enter; a name that's taken says so.
  const name = page.getByTestId("file-name");
  await name.fill("inputs");
  await name.press("Enter");
  await expect(page.getByTestId("files-panel")).toContainText("there's already a file called inputs");
  await name.fill("protocol");
  await name.press("Enter");
  await expect(selector).toHaveText("protocol");
  await page.getByTestId("file-description").fill("A test protocol");

  await page.getByTestId("file-duplicate").click();
  await expect(selector).toHaveText("protocol-copy");
  await expect(page.getByTestId("file-description")).toHaveValue("A test protocol"); // copied

  // The Run page uses the selected file.
  await page.getByTestId("nav-run").click();
  await expect(page.getByLabel("Input file")).toHaveText("protocol-copy");
  await page.getByTestId("nav-inputs").click();

  // Deleting asks first; the file before it is selected next.
  await page.getByTestId("file-delete").click();
  await page.getByTestId("file-delete-confirm").click();
  await expect(selector).toHaveText("protocol");
  await page.getByTestId("file-delete").click();
  await page.getByTestId("file-delete-confirm").click();
  await expect(selector).toHaveText("inputs");
});

test("sources, trials and previews; built, run and fired from the Run page; locked while running", async ({ page }) => {
  test.setTimeout(90_000);
  await openRing(page);
  await page.getByTestId("nav-inputs").click();
  const editor = page.getByTestId("source-editor");

  // A Poisson source.
  await page.getByTestId("add-poisson").click();
  await page.getByLabel("Name", { exact: true }).fill("background");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-1");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  // A pulse source, with two times.
  await page.getByTestId("add-pulse").click();
  await page.getByLabel("Name", { exact: true }).fill("tap");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-10");
  await page.getByTestId("pulse-times").fill("500 ms, 1 s");
  // Times that aren't times say so.
  await page.getByTestId("pulse-times").fill("soon");
  await expect(editor).toContainText("must be a time");
  await page.getByTestId("pulse-times").fill("500 ms, 1 s");
  // A signal source.
  await page.getByTestId("add-signal").click();
  await page.getByLabel("Name", { exact: true }).fill("wave");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-1");
  await page.getByTestId("formula").fill("sin(2 * pi * t / 200 + c)");
  await page.getByLabel("Duration").fill("1 s");
  await expect(page.getByTestId("source-list").locator("[data-testid^=source-row-]")).toHaveCount(3);

  // The timeline: a bar for the poisson source and the signal, a tick for each pulse; the signal's channels.
  const timeline = page.getByTestId("timeline");
  await expect(timeline.getByTestId("timeline-row-tap").getByTestId("timeline-mark")).toHaveCount(2);
  await expect(timeline.getByTestId("timeline-row-wave").getByTestId("timeline-span")).toHaveCount(1);
  await expect(timeline.getByTestId("timeline-row-background").getByTestId("timeline-span")).toHaveCount(1);
  await expect(page.getByTestId("preview").locator("canvas")).toBeVisible();
  // Without a built network, the targets can't be shown.
  await expect(page.getByTestId("targets-hint")).toBeVisible();

  // Trials: the signal runs in each of 3 trials.
  await page.getByTestId("trials-toggle").click();
  await page.getByTestId("trials-count").fill("3");
  await page.getByTestId("trials-length").fill("2 s");
  await page.getByTestId("trials-gap").fill("uniform(200 ms, 400 ms)");
  await page.getByTestId("source-row-wave").getByRole("button", { name: "Select wave" }).click();
  await page.getByTestId("source-scope").click();
  await page.getByRole("option", { name: "in each trial" }).click();
  await expect(timeline.getByTestId("timeline-trial")).toHaveCount(3);
  await expect(timeline.getByTestId("timeline-row-wave").getByTestId("timeline-span")).toHaveCount(3);
  await page.screenshot({ path: "test-results/inputs-page.png", fullPage: true });

  // The Run page: the pulse source can be fired once the network is built with this file.
  await page.getByTestId("nav-run").click();
  await expect(page.getByTestId("input-summary")).toHaveText("3 of 3 sources on");
  await expect(page.getByTestId("fire-tap")).toBeDisabled();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await expect(page.getByTestId("fire-tap")).toBeEnabled();
  // The targets are shown in 3D, with their counts in the list.
  await page.getByTestId("edit-inputs").click();
  await expect(page.getByTestId("targets-view").locator("canvas")).toBeVisible();
  await expect(page.getByTestId("source-count-tap")).toContainText("5 neurons");

  // A run: the file is read-only, sources show their state, the trial shows, Fire works.
  await page.getByTestId("nav-run").click();
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  await page.getByTestId("nav-inputs").click();
  await expect(page.getByTestId("locked-note")).toBeVisible();
  await expect(page.getByTestId("add-poisson")).toBeDisabled();
  await expect(page.getByTestId("source-name")).toBeDisabled();
  await expect(page.getByLabel("Hz per neuron", { exact: true })).toBeDisabled();
  await expect(page.getByTestId("source-state-background")).toHaveText("on");
  await expect(page.getByTestId("inputs-trial")).toContainText(/1 of 3|gap/);
  await page.screenshot({ path: "test-results/inputs-running.png", fullPage: true });
  await expect(page.getByTestId("fire-tap")).toBeEnabled(); // Fire stays available (it's tested below)

  // Stopped: editable again.
  await page.getByTestId("nav-run").click();
  await page.getByTestId("stop").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await page.getByTestId("nav-inputs").click();
  await expect(page.getByTestId("add-poisson")).toBeEnabled();
  await expect(page.getByLabel("Hz per neuron", { exact: true })).toBeEnabled();
});

test("Fire sends a pulse source's pulse now, in a running network", async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await openRing(page);
  // A pulse source with no times: nothing else drives the ring.
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-pulse").click();
  await page.getByLabel("Name", { exact: true }).fill("tap");
  await page.getByLabel("Target neurons", { exact: true }).fill("hidden-101");
  await page.getByTestId("pulse-times").fill("");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  const session = Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
  const client = await WebSocketClient.connect(baseURL!);
  const target = (await client.getNetwork(session)).neurons.find((n) => n.name === "hidden-101")!.index;
  await page.waitForTimeout(500);
  expect((await client.queryNeuron(session, target)).spikes).toBe(0);
  await page.getByTestId("fire-tap").click(); // from the Run page
  await expect.poll(async () => (await client.queryNeuron(session, target)).spikes, { timeout: 5000 }).toBeGreaterThan(0);
  client.close();
  await page.getByTestId("stop").click();
});
