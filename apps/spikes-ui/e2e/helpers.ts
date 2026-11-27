import { expect, type Page } from "@playwright/test";

/** Adds a Poisson source named "background" to the project's input file (on the Inputs page), and returns to the Run
 *  page. `neurons`: the target prefix (default: all neurons). */
export async function addBackground(page: Page, { neurons, hz, mv }: { neurons?: string; hz?: number; mv?: number }) {
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-poisson").click();
  await page.getByLabel("Name", { exact: true }).fill("background");
  if (neurons !== undefined) await page.getByLabel("Target neurons", { exact: true }).fill(neurons);
  if (hz !== undefined) await page.getByLabel("Hz per neuron", { exact: true }).fill(String(hz));
  if (mv !== undefined) await page.getByLabel("mV", { exact: true }).fill(String(mv));
  await expect(page.getByTestId("source-row-background")).toBeVisible();
  await page.getByTestId("nav-run").click();
}

/** Adds a pulse source named `name` (no times: it's fired by hand) targeting the neurons starting with `neurons`. */
export async function addPulse(page: Page, { name, neurons, mv }: { name: string; neurons: string; mv?: number }) {
  await page.getByTestId("nav-inputs").click();
  await page.getByTestId("add-pulse").click();
  await page.getByLabel("Name", { exact: true }).fill(name);
  await page.getByLabel("Target neurons", { exact: true }).fill(neurons);
  await page.getByLabel("Pulse times (from the start)").fill("");
  if (mv !== undefined) await page.getByLabel("mV", { exact: true }).fill(String(mv));
  await expect(page.getByTestId(`source-row-${name}`)).toBeVisible();
  await page.getByTestId("nav-run").click();
}
