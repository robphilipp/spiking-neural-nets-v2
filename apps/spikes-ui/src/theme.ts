// Light and dark mode (shadcn/ui's `.dark` class on <html>), and colours for the charts and the 3D view, read from
// the theme's CSS variables so they follow the mode.

import { useEffect, useState } from "react";

export type ThemeChoice = "system" | "light" | "dark";

const dark = typeof window !== "undefined" ? window.matchMedia?.("(prefers-color-scheme: dark)") : undefined;
let choice: ThemeChoice = "system";

function update() {
  const on = choice === "dark" || (choice === "system" && !!dark?.matches);
  document.documentElement.classList.toggle("dark", on);
  document.documentElement.style.colorScheme = on ? "dark" : "light";
}
dark?.addEventListener?.("change", () => choice === "system" && update());

/** Applies the user's choice (and, for "system", follows the system's from now on). */
export function applyTheme(next: ThemeChoice) {
  choice = next;
  update();
}

/** The mode in effect, following changes. */
export function useThemeMode(): "light" | "dark" {
  const read = () => (document.documentElement.classList.contains("dark") ? "dark" : "light");
  const [mode, setMode] = useState<"light" | "dark">(read);
  useEffect(() => {
    const observer = new MutationObserver(() => setMode(read()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);
  return mode;
}

/** A CSS variable's colour as `rgb(…)`/`rgba(…)`: the theme uses oklch(), which the chart library and three.js
 *  don't all understand, so it's drawn into a pixel and read back. */
export function cssColor(variable: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(variable).trim() || "#888";
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return value;
  ctx.fillStyle = value;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
  return a === 255 ? `rgb(${r}, ${g}, ${b})` : `rgba(${r}, ${g}, ${b}, ${((a ?? 255) / 255).toFixed(3)})`;
}

/** Colours for stream-charts, from the theme, so charts follow light and dark mode. */
export interface ChartColors {
  color: string;
  backgroundColor: string;
  series: string;
  highlight: string;
  tooltip: { fontColor: string; backgroundColor: string; borderColor: string };
}

function chartColors(): ChartColors {
  return {
    color: cssColor("--muted-foreground"),
    backgroundColor: cssColor("--card"),
    series: cssColor("--series"),
    highlight: cssColor("--highlight"),
    tooltip: { fontColor: cssColor("--popover-foreground"), backgroundColor: cssColor("--popover"), borderColor: cssColor("--border") },
  };
}

/** Chart colours for the current mode. */
export function useChartColors(): ChartColors {
  const mode = useThemeMode();
  const [colors, setColors] = useState(chartColors);
  useEffect(() => setColors(chartColors()), [mode]);
  return colors;
}
