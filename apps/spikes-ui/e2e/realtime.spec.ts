import { expect, type Page, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";
import { addBackground, addRealtime, audioRuns } from "./helpers";

// Realtime sources (app plan 05, phase I4) on the line-sensor example: 49 input neurons named input-11 … input-77, in
// seven groups ("input-1" … "input-7") of seven. Chromium's fake devices give a steady 440 Hz tone and a moving test
// pattern (see playwright.config.ts).

async function openLineSensor(page: Page) {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "line-sensor-layers-wta", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
}

async function build(page: Page) {
  await page.getByTestId("nav-run").click();
  await page.getByTestId("build").click();
  await expect(page.getByText("69 neurons")).toBeVisible();
}

async function sessionId(page: Page): Promise<number> {
  return Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
}

/** Total spikes of the neurons whose names start with `prefix`, asked of the engine. */
async function spikesOf(client: WebSocketClient, session: number, prefix: string): Promise<number> {
  const network = await client.getNetwork(session);
  let total = 0;
  for (const n of network.neurons.filter((x) => x.name.startsWith(prefix))) total += (await client.queryNeuron(session, n.index)).spikes;
  return total;
}

const state = (page: Page, name: string) => page.getByTestId(`rt-run-state-${name}`);

test("a microphone, a camera and a sensor stream at once into the network", async ({ page, baseURL }) => {
  test.setTimeout(90_000);
  await openLineSensor(page);
  await build(page); // (to see what the sources send; they're given to the run when it starts)
  await addRealtime(page, "microphone", { name: "voice", neurons: "input-1", fill: (e) => e.getByLabel("Hz at 1").fill("400") });
  await addRealtime(page, "camera", {
    name: "eye",
    neurons: "input-3",
    fill: async (e) => {
      await e.getByLabel("Grid (columns×rows)").fill("3x2");
    },
  });
  await addRealtime(page, "sensor", { name: "arm", neurons: "input-5" });
  await expect(page.getByTestId("channel-plan")).toBeVisible();
  await page.screenshot({ path: "test-results/realtime-inputs.png", fullPage: true });
  await build(page);
  await expect(page.getByTestId("input-summary")).toContainText("3 realtime");
  await page.getByTestId("start").click();
  await expect(page.getByTestId("state")).toHaveText("running");
  const session = await sessionId(page);

  // The microphone and the camera stream by themselves; the sensor waits for a program.
  await expect(state(page, "voice")).toHaveText("streaming", { timeout: 15_000 });
  await expect(state(page, "eye")).toHaveText("streaming", { timeout: 15_000 });
  await expect(state(page, "arm")).toHaveText("starting");
  await expect(page.getByTestId("rt-run-arm")).toContainText("waiting for a program to send to the sensor named 'arm'");
  await expect(page.getByTestId("input-summary")).toContainText("2 streaming");

  // A program claims the sensor by name and sends samples to it: 7 channels at 100 Hz for a while.
  const program = await WebSocketClient.connect(baseURL!);
  const arm = await program.claimSensor(session, "arm");
  const t0 = Date.now();
  for (let i = 0; i < 150; i++) {
    const values = new Float32Array(7 * 10).fill(1); // 10 samples, 1 ms apart
    arm.sendSamples(BigInt(i * 10_000_000), 1_000_000, 7, values);
    await page.waitForTimeout(Math.max(0, t0 + (i + 1) * 10 - Date.now()));
  }
  await expect(state(page, "arm")).toHaveText("streaming", { timeout: 5000 });

  // The engine sees all three, with samples, and the neurons they drive spike.
  const sensors = await program.listSensors(session);
  expect(sensors.map((s) => s.name).sort()).toEqual(["arm", "eye", "voice"]);
  for (const s of sensors) expect(s.samples, s.name).toBeGreaterThan(0);
  expect(sensors.find((s) => s.name === "eye")!.channels).toHaveLength(6); // 3×2 cells, a neuron each
  expect(sensors.find((s) => s.name === "arm")!.channels).toHaveLength(7);
  expect(await spikesOf(program, session, "input-5")).toBeGreaterThan(0);
  expect(await spikesOf(program, session, "input-3")).toBeGreaterThan(0);
  if (await audioRuns(page)) expect(await spikesOf(program, session, "input-1")).toBeGreaterThan(0);

  await page.getByTestId("realtime-status").screenshot({ path: "test-results/realtime-run-status.png" });
  // The Inputs page shows each source's status in its row while the run goes.
  await page.getByTestId("nav-inputs").click();
  await expect(page.getByTestId("rt-state-voice")).toHaveText("streaming");
  await expect(page.getByTestId("rt-detail-eye")).toContainText(/samples\/s · [\d,]+ samples · \d+ late · \d+ dropped/);
  await page.screenshot({ path: "test-results/realtime-inputs-running.png", fullPage: true });

  // Ending the run stops them all, and the engine forgets the sensors (the program's claim on 'arm' is the last).
  await page.getByTestId("nav-run").click();
  await page.getByTestId("stop").click();
  await expect(state(page, "voice")).toHaveText("stopped");
  await expect(state(page, "eye")).toHaveText("stopped");
  await expect(state(page, "arm")).toHaveText("stopped");
  await arm.disconnect();
  await expect.poll(async () => (await program.listSensors(session)).length).toBe(0);
  program.close();
});

test("a run as fast as possible takes no realtime input but runs the simulated sources", async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await openLineSensor(page);
  await addBackground(page, { neurons: "input-", hz: 50, mv: 12 });
  await addRealtime(page, "microphone", { name: "voice", neurons: "input-1" });
  await build(page);
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("1");
  await page.getByLabel("Run for (s)").blur();
  await expect(page.getByTestId("realtime-asap-note")).toContainText("voice");
  await page.getByTestId("start").click();
  await expect(state(page, "voice")).toHaveText("refused");
  await expect(page.getByTestId("rt-run-voice")).toContainText("as fast as possible");
  await page.screenshot({ path: "test-results/realtime-asap.png", fullPage: true });
  const session = await sessionId(page);
  const client = await WebSocketClient.connect(baseURL!);
  await expect.poll(async () => (await client.listSessions()).find((s) => s.id === session), { timeout: 15_000 }).toMatchObject({ state: "ready", simMs: 1000 });
  expect(await client.listSensors(session)).toHaveLength(0);
  expect(await spikesOf(client, session, "input-")).toBeGreaterThan(0); // the background input ran
  client.close();
});

test("speed = refuse is refused at ×0.5 and runs at ×1", async ({ page }) => {
  test.setTimeout(60_000);
  await openLineSensor(page);
  await addRealtime(page, "microphone", { name: "voice", neurons: "input-1", fill: (e) => e.getByTestId("speed-rule").click() });
  await page.getByRole("option", { name: "refuse: don't start" }).click();
  await addRealtime(page, "microphone", { name: "voice2", neurons: "input-2" });
  await build(page);
  // ×1/2: the slider's one step left of ×1.
  await page.getByRole("slider").press("ArrowLeft");
  await expect(page.getByText("Speed ×1/2")).toBeVisible();
  await page.getByTestId("start").click();
  await expect(state(page, "voice")).toHaveText("refused", { timeout: 10_000 });
  await expect(page.getByTestId("rt-run-voice")).toContainText("the run is at speed ×0.5 and this source runs only at ×1");
  await expect(state(page, "voice2")).toHaveText("streaming", { timeout: 15_000 }); // resampled: its time scaled by 0.5
  await page.getByTestId("stop").click();

  // At ×1 it runs.
  await page.getByRole("slider").press("ArrowRight");
  await expect(page.getByText("Speed ×1", { exact: true })).toBeVisible();
  await page.getByTestId("start").click();
  await expect(state(page, "voice")).toHaveText("streaming", { timeout: 15_000 });
  await page.getByTestId("stop").click();
});

test("while paused a source drops what arrives, or buffers it", async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await openLineSensor(page);
  await addRealtime(page, "microphone", { name: "dropper", neurons: "input-1" });
  await addRealtime(page, "microphone", {
    name: "keeper",
    neurons: "input-2",
    fill: (e) => e.getByTestId("on-pause").click(),
  });
  await page.getByRole("option", { name: "buffer it for the resume" }).click();
  await build(page);
  await page.getByTestId("start").click();
  await expect(state(page, "dropper")).toHaveText("streaming", { timeout: 15_000 });
  await expect(state(page, "keeper")).toHaveText("streaming", { timeout: 15_000 });
  const session = await sessionId(page);
  await page.getByTestId("pause").click();
  await expect(page.getByTestId("state")).toHaveText("paused");
  const client = await WebSocketClient.connect(baseURL!);
  await expect
    .poll(async () => (await client.listSensors(session)).find((s) => s.name === "dropper")!.dropped, { timeout: 10_000 })
    .toBeGreaterThan(50);
  const sensors = await client.listSensors(session);
  expect(sensors.find((s) => s.name === "keeper")!.dropped).toBe(0);
  // The Run page shows the dropped samples, and the streams carry on after the resume.
  await expect(page.getByTestId("rt-run-dropper")).toContainText(/[1-9]\d* dropped/, { timeout: 5000 });
  await expect(page.getByTestId("rt-run-keeper")).toContainText("0 dropped");
  await page.getByRole("button", { name: "Resume" }).click();
  await expect(page.getByTestId("state")).toHaveText("running");
  const before = (await client.listSensors(session)).find((s) => s.name === "dropper")!.samples;
  await expect.poll(async () => (await client.listSensors(session)).find((s) => s.name === "dropper")!.samples).toBeGreaterThan(before);
  await page.getByTestId("stop").click();
  client.close();
});

test("a camera with too few target neurons says so and doesn't start; the others do", async ({ page }) => {
  test.setTimeout(60_000);
  await openLineSensor(page);
  // 16x16 needs 256 target neurons; the 49 input neurons are too few.
  await addRealtime(page, "camera", { name: "eye", neurons: "input-" });
  await addRealtime(page, "sensor", { name: "arm", neurons: "input-5" });
  await build(page);
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("source-row-eye").click();
  await expect(page.getByTestId("source-channels-eye")).toHaveText("camera 16x16 needs 256 target neurons, has 49");
  await expect(page.getByTestId("channel-plan")).toContainText("The source won't start");
  await page.getByTestId("nav-run").click();
  await page.getByTestId("start").click();
  await expect(state(page, "eye")).toHaveText("error");
  await expect(page.getByTestId("rt-run-eye")).toContainText("camera 16x16 needs 256 target neurons, has 49");
  await expect(state(page, "arm")).toHaveText("starting");
  await page.getByTestId("stop").click();
});

test("the Inputs page previews what a microphone and a camera send", async ({ page }) => {
  test.setTimeout(60_000);
  await openLineSensor(page);
  await build(page);
  await addRealtime(page, "microphone", { name: "voice", neurons: "input-1" });
  await page.getByTestId("preview-toggle").click();
  await expect(page.getByTestId("preview-bars").locator("div")).toHaveCount(7); // a band per target neuron
  await page.screenshot({ path: "test-results/realtime-preview-mic.png", fullPage: true });
  await page.getByTestId("preview-toggle").click();
  await addRealtime(page, "camera", { name: "eye", neurons: "input-", fill: (e) => e.getByLabel("Grid (columns×rows)").fill("6x6") });
  await page.getByTestId("preview-toggle").click();
  const canvas = page.getByTestId("preview-canvas");
  await expect(canvas).toBeVisible();
  // The fake camera's pattern changes: the canvas isn't blank.
  await expect
    .poll(async () => canvas.evaluate((c: HTMLCanvasElement) => c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data.some((v, i) => i % 4 !== 3 && v > 0)))
    .toBe(true);
  await page.getByTestId("camera-mode").click();
  await page.getByRole("option", { name: "changes (on and off)" }).click();
  await expect(page.getByTestId("channel-plan")).toContainText("camera 6x6 changes (on and off) needs 72 target neurons, has 49");
  await page.screenshot({ path: "test-results/realtime-preview-camera.png", fullPage: true });
});
