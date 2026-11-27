import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

const TRIALS = fileURLToPath(new URL("../../../test-data/projects/ring/trials.inputs.toml", import.meta.url));

test("an input file's trials run, show on the Run page, and reach the engine", async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "ring", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");

  // Open the input file (the browser has no project folders).
  await page.getByTestId("nav-projects").click();
  await page.getByTestId("inputs-file-input").setInputFiles(TRIALS);
  await expect(page.getByText("Opened trials.inputs.toml")).toBeVisible();

  await page.getByTestId("nav-run").click();
  await expect(page.getByLabel("Input file")).toHaveText("trials");
  await expect(page.getByTestId("input-summary")).toHaveText("3 of 3 sources on");
  await page.getByTestId("build").click();
  await expect(page.getByTestId("state")).toHaveText("ready");
  const trial = page.getByTestId("trial");
  await expect(trial).toContainText("starts at 0.2 s");

  // Running: a trial and its condition, or the gap before the next.
  await page.getByTestId("start").click();
  await expect(trial).toContainText(/\d of 8 · column [13]|gap · next \d/, { timeout: 5000 });
  await page.getByTestId("timing").screenshot({ path: "test-results/trials.png" });
  await page.getByTestId("stop").click();

  // The engine has the same trials: 8, four per condition, in order.
  const session = Number(/session (\d+)/.exec(await page.getByTestId("session-picker").innerText())![1]);
  const client = await WebSocketClient.connect(baseURL!);
  const trials = await client.getTrials(session);
  expect(trials).toHaveLength(8);
  expect(trials.filter((t) => t.condition === "column 1")).toHaveLength(4);
  for (let k = 1; k < trials.length; k++) expect(trials[k]!.startMs).toBeGreaterThanOrEqual(trials[k - 1]!.endMs);

  // To the end: done.
  await page.getByLabel("As fast as possible").check();
  await page.getByLabel("Run for (s)").fill("13");
  await page.getByLabel("Run for (s)").blur();
  await page.getByTestId("start").click();
  await expect(trial).toHaveText(/done/, { timeout: 30_000 });
  client.close();
});
