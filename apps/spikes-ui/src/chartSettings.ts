// How the live charts take in and keep data, as in stream-charts' examples: the chart's buffering (windowing time),
// how often the engine sends data (the update period), whether the time axis moves smoothly (cadence), and how
// much simulated time is kept. Remembered in this browser; the live views follow changes as they happen.

import { useMemo } from "react";
import { create } from "zustand";
import { recall, remember, useSession } from "./store";

export interface ChartSettings {
  /** How long a chart gathers incoming data before drawing it (its windowing time), wall ms. */
  bufferMs: number;
  /** How often the engine sends the charts' data (frames), wall ms. */
  updateMs: number;
  /** Move the time axes on every animation frame, so charts scroll smoothly between updates. */
  cadence: boolean;
  /** How much simulated time the charts keep, ms. */
  keepMs: number;
}

export const DEFAULT_CHART_SETTINGS: ChartSettings = {
  bufferMs: 50,
  updateMs: 50,
  cadence: false,
  keepMs: 100_000,
};

/** Choices for how much simulated time to keep. */
export const KEEP_CHOICES = [10_000, 30_000, 60_000, 100_000, 300_000];

const KEY = "spikes.charts";

type ChartSettingsStore = ChartSettings & {
  setBufferMs: (bufferMs: number) => void;
  setUpdateMs: (updateMs: number) => void;
  setCadence: (cadence: boolean) => void;
  setKeepMs: (keepMs: number) => void;
  reset: () => void;
  /** Bumped to put every chart's zoom and pan back (charts are keyed on it). Not remembered. */
  zoomResets: number;
  resetZoom: () => void;
  /** The charts hold still while the run goes on; on unfreezing, they take in what came meanwhile. Not remembered. */
  frozen: boolean;
  setFrozen: (frozen: boolean) => void;
};

export const useChartSettings = create<ChartSettingsStore>()((set) => ({
  ...DEFAULT_CHART_SETTINGS,
  ...recall<Partial<ChartSettings>>(KEY, {}),
  setBufferMs: (bufferMs) => set({ bufferMs }),
  setUpdateMs: (updateMs) => set({ updateMs }),
  setCadence: (cadence) => set({ cadence }),
  setKeepMs: (keepMs) => set({ keepMs }),
  reset: () => set(DEFAULT_CHART_SETTINGS),
  zoomResets: 0,
  resetZoom: () => set((s) => ({ zoomResets: s.zoomResets + 1 })),
  frozen: false,
  setFrozen: (frozen) => set({ frozen }),
}));

useChartSettings.subscribe(({ bufferMs, updateMs, cadence, keepMs }) =>
  remember(KEY, { bufferMs, updateMs, cadence, keepMs }),
);

/** Every plot can be panned (drag) and zoomed (shift or ctrl + scroll, so plain scrolling still scrolls the page). */
export const ZOOM = {
  panEnabled: true,
  zoomEnabled: true,
  zoomKeyModifiersRequired: true,
} as const;

/** How to zoom and pan, for help text. */
export const ZOOM_HELP = "Drag a chart to pan; hold shift and scroll to zoom.";

/**
 * A live chart's time axis when it mounts: the window that ends at the current simulated time. A chart remounts when
 * its zoom is reset or the time window or cadence changes; starting at [0, window] would then show the start of a
 * run that has ended, since no new data comes to scroll it forward.
 */
export function useTimeDomain(windowMs: number): [number, number] {
  const cadence = useChartSettings((s) => s.cadence);
  const zoomResets = useChartSettings((s) => s.zoomResets);
  return useMemo(() => {
    const now = useSession.getState().simMs;
    return now > windowMs ? [now - windowMs, now] : [0, windowMs];
    // eslint-disable-next-line react-hooks/exhaustive-deps -- recomputed whenever the charts remount
  }, [windowMs, cadence, zoomResets]);
}

/** The current settings (outside React). */
export const chartSettings = (): ChartSettings => useChartSettings.getState();
