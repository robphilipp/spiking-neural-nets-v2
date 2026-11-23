import { expect, test } from "@playwright/test";
import { WebSocketClient } from "@spikes/client";

// A3 acceptance: a 100k-neuron network at ~5 Hz, paced at real time, with the raster (300 rows) and the
// population rate drawing live; the page keeps its frame rate and the simulation keeps up with the clock.
test("100k neurons at real time: the charts keep up", async ({ page, baseURL }) => {
  test.setTimeout(180_000);
  const n = 100_000;
  const client = await WebSocketClient.connect(baseURL!);
  const neurons = Array.from(
    { length: n },
    (_, i) => `(nid=n${i}, mst=10 mV, mpn=0 mV, LOC=(cst=ct, px1=${(i % 100) * 10}, px2=${(Math.floor(i / 100) % 100) * 10}, px3=${Math.floor(i / 10_000) * 10}))`,
  ).join(",\n");
  const { id } = await client.createSession("scale-100k", `(NRN=[${neurons}])`);
  await client.build(id, { shards: 4 });
  const all = Array.from({ length: n }, (_, i) => i);
  await client.addInput(id, { type: "poisson", targets: all, rateHz: 5 * n, mv: 12, durationMs: null });
  await client.start(id, { speed: 1 });

  try {
    await page.goto("/");
    await expect(page.getByTestId("connection")).toHaveText("connected");
    await page.getByTestId("session-picker").click();
    await page.getByRole("option", { name: /scale-100k/ }).click();
    await page.getByTestId("nav-run").click();
    await expect(page.getByText("100,000 neurons")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("raster").locator("canvas")).toBeVisible();
    await page.waitForTimeout(2000); // let the charts fill

    // The page's own frame rate over 5 s, while both charts draw.
    const fps = await page.evaluate(
      () =>
        new Promise<number>((resolve) => {
          let frames = 0;
          const t0 = performance.now();
          const tick = () => {
            frames++;
            if (performance.now() - t0 < 5000) requestAnimationFrame(tick);
            else resolve(frames / ((performance.now() - t0) / 1000));
          };
          requestAnimationFrame(tick);
        }),
    );
    const timing = await page.getByTestId("timing").innerText();
    await page.screenshot({ path: "test-results/scale-100k.png" });
    await page.getByTestId("network-3d").screenshot({ path: "test-results/scale-100k-3d.png" });
    console.log(`page ${fps.toFixed(1)} fps; ${timing.replace(/\n/g, " ")}`);

    expect(fps).toBeGreaterThan(50);
    const framesPerS = Number(/Frames\s+(\d+)\/s/.exec(timing)?.[1]);
    expect(framesPerS).toBeGreaterThan(50);
    const lagMs = Number(/Behind the clock\s+([\d.]+) ms/.exec(timing)?.[1]);
    expect(lagMs).toBeLessThan(50);
  } finally {
    await client.stop(id);
    await client.closeSession(id);
    client.close();
  }
});
