import { expect, test } from "@playwright/test";

test("errors the engine reports outside any request are shown, counted and dismissable", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await expect(page.getByTestId("engine-error")).toHaveCount(0);

  // A malformed input batch, sent as the Inputs page sends its batches: the engine rejects it without a request id.
  const sendBad = () =>
    page.evaluate(() => {
      const client = (
        window as unknown as {
          __spikesClient: { sendBinary(data: ArrayBuffer): void };
        }
      ).__spikesClient;
      client.sendBinary(new Uint8Array([1, 2, 3]).buffer);
    });
  await sendBad();
  const banner = page.getByTestId("engine-error");
  await expect(banner).toContainText("The engine reported an error");
  await sendBad();
  await expect(banner).toContainText("(2 times)");

  // Visible on every page, until dismissed.
  await page.getByTestId("nav-inputs").click();
  await expect(banner).toBeVisible();
  await banner.getByRole("button").click();
  await expect(banner).toHaveCount(0);
});
