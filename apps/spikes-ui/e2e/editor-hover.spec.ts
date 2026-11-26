import { expect, test } from "@playwright/test";

test("hover help near the editor's edges isn't cut off", async ({ page }) => {
  // A short editor (a small window, the 3D view open): too little room below the line for the help.
  await page.setViewportSize({ width: 1200, height: 640 });
  await page.goto("/");
  await expect(page.getByTestId("connection")).toHaveText("connected");
  await page.getByTestId("nav-network").click();
  const toggle = page.getByTestId("network-view-toggle");
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  await page.locator(".monaco-editor").click();
  await page.keyboard.press("ControlOrMeta+A");
  // A long help entry (dynamics) near the top of a short editor: there's room for it neither above the line nor
  // below it, inside the editor.
  await page.keyboard.insertText("(\nneurons=[\n(name=a, dynamics=monostable_resonator, position=[0, 0, 0])])");
  const help = page.locator(".monaco-hover").filter({ hasText: "leaky integrate-and-fire" });
  await expect(async () => {
    await page.mouse.move(0, 0);
    await page.getByText("dynamics", { exact: true }).hover();
    await expect(help).toBeVisible({ timeout: 1500 });
  }).toPass({ timeout: 10_000 });
  await page.screenshot({ path: "test-results/editor-hover-top.png" });

  // Every edge of the help is on screen, and nothing that holds it clips it.
  const clipped = await help.first().evaluate((el) => {
    const box = el.getBoundingClientRect();
    if (box.top < 0 || box.left < 0 || box.bottom > innerHeight || box.right > innerWidth) return "off screen";
    // An ancestor's overflow clips the help unless something in between is fixed-position (which escapes overflow
    // clipping, short of a transformed or contained ancestor).
    let fixed = getComputedStyle(el).position === "fixed";
    for (let p = el.parentElement; p; p = p.parentElement) {
      const style = getComputedStyle(p);
      const traps = style.transform !== "none" || style.filter !== "none" || /paint|strict|content/.test(style.contain);
      if (fixed && !traps) {
        continue;
      }
      if (style.position === "fixed") fixed = true;
      if (style.overflow === "visible" && style.overflowY === "visible") continue;
      const r = p.getBoundingClientRect();
      if (box.top < r.top - 1 || box.bottom > r.bottom + 1) return `clipped by ${p.className || p.tagName}`;
    }
    return null;
  });
  expect(clipped).toBeNull();
});
