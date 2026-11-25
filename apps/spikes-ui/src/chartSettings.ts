// How the live charts take in and keep data, as in stream-charts' examples: the chart's buffering (windowing time),
// how often the engine sends data (the update period), whether the time axis moves smoothly (cadence), and how
// much simulated time is kept. Remembered in this browser; the live views follow changes as they happen.

import { create } from "zustand";
import { recall, remember } from "./store";

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

export const DEFAULT_CHART_SETTINGS: ChartSettings = { bufferMs: 50, updateMs: 50, cadence: false, keepMs: 100_000 };

/** Choices for how much simulated time to keep. */
export const KEEP_CHOICES = [10_000, 30_000, 60_000, 100_000, 300_000];

const KEY = "spikes.charts";

type ChartSettingsStore = ChartSettings & {
  setBufferMs: (bufferMs: number) => void;
  setUpdateMs: (updateMs: number) => void;
  setCadence: (cadence: boolean) => void;
  setKeepMs: (keepMs: number) => void;
  reset: () => void;
};

export const useChartSettings = create<ChartSettingsStore>()((set) => ({
  ...DEFAULT_CHART_SETTINGS,
  ...recall<Partial<ChartSettings>>(KEY, {}),
  setBufferMs: (bufferMs) => set({ bufferMs }),
  setUpdateMs: (updateMs) => set({ updateMs }),
  setCadence: (cadence) => set({ cadence }),
  setKeepMs: (keepMs) => set({ keepMs }),
  reset: () => set(DEFAULT_CHART_SETTINGS),
}));

useChartSettings.subscribe(({ bufferMs, updateMs, cadence, keepMs }) => remember(KEY, { bufferMs, updateMs, cadence, keepMs }));

/** The current settings (outside React). */
export const chartSettings = (): ChartSettings => useChartSettings.getState();
