import { expect, test } from "@playwright/test";

test("a network of populations and connection rules builds and runs", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  await page.getByLabel("Open an example").click();
  await page.getByRole("option", { name: "populations", exact: true }).click();
  await page.getByTestId("apply").click();
  await expect(page.getByTestId("network-status")).toHaveText("Applied");
  await page.getByTestId("nav-run").click();
  await page.getByLabel("Neurons starting with", { exact: true }).fill("exc-");
  await page.getByLabel("Hz per neuron", { exact: true }).fill("20");
  await page.getByTestId("background").check();
  await page.getByTestId("build").click();
  // Every neuron and connection was generated from 2 populations and 3 rules.
  const header = page.getByText(/4,000 neurons · [\d,]+ synapses/);
  await expect(header).toBeVisible({ timeout: 15_000 });
  const synapses = Number(/· ([\d,]+) synapses/.exec((await header.innerText()) ?? "")![1]!.replace(/,/g, ""));
  expect(synapses).toBeGreaterThan(190_000); // 3200 × 50 + 800 × 40, plus the inhibitory ones nearby
  await page.getByTestId("start").click();
  await expect(page.getByTestId("timing")).toContainText(/Spikes\s*[1-9][\d,]*\/s/, { timeout: 5000 });
  await page.getByTestId("stop").click();
});
