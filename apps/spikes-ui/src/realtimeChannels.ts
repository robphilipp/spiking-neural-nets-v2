// What realtime sources send (app plan 05, phase I4): how many channels a source has and how a microphone's spectrum
// and a camera's frames become channel values. Pure functions, so the capture glue (capture.ts) stays thin and the
// Inputs page's previews use exactly what the run sends.

import { type CameraMode, DEFAULT_GRID, type Grid, type RealtimeSource } from "./inputFile";

/** Bands in a microphone preview when no network says how many neurons it drives. */
export const PREVIEW_BANDS = 16;

/** The lowest and highest frequency of a microphone's bands (Hz): speech and most sound of interest. */
const LOW_HZ = 100;
const HIGH_HZ = 8000;
/** The spectrum level (dB) shown as 0 and as 1. */
const FLOOR_DB = -90;
const CEIL_DB = -30;

/**
 * An analyser's spectrum (dB per bin, from 0 Hz to half the sample rate) in `channels` log-spaced bands from 100 Hz
 * up to 8 kHz (or the Nyquist frequency if lower), each the mean level of its bins, scaled so −90 dB is 0 and −30 dB
 * is 1.
 */
export function bandLevels(spectrum: ArrayLike<number>, channels: number, sampleRate: number): Float32Array {
  const out = new Float32Array(channels);
  const binHz = sampleRate / 2 / spectrum.length;
  const lo = Math.log(LOW_HZ);
  const hi = Math.log(Math.min(HIGH_HZ, sampleRate / 2));
  for (let c = 0; c < channels; c++) {
    const from = Math.max(0, Math.floor(Math.exp(lo + ((hi - lo) * c) / channels) / binHz));
    const to = Math.max(from + 1, Math.floor(Math.exp(lo + ((hi - lo) * (c + 1)) / channels) / binHz));
    let sum = 0;
    let n = 0;
    for (let b = from; b < to && b < spectrum.length; b++, n++) sum += Math.max(FLOOR_DB, spectrum[b]!); // −∞ dB: silence
    out[c] = n === 0 ? 0 : Math.min(1, Math.max(0, (sum / n - FLOOR_DB) / (CEIL_DB - FLOOR_DB)));
  }
  return out;
}

/**
 * RGBA pixels (`width` × `height`) as brightness (0–1, Rec. 709 weights) averaged over a `grid`'s cells, row by row.
 * Any image size works: a cell takes the pixels in its share of the image (at least one).
 */
export function lumaGrid(rgba: ArrayLike<number>, width: number, height: number, grid: Grid): Float32Array {
  const out = new Float32Array(grid.cols * grid.rows);
  for (let r = 0; r < grid.rows; r++) {
    const y0 = Math.min(height - 1, Math.floor((r * height) / grid.rows));
    const y1 = Math.min(height, Math.max(y0 + 1, Math.floor(((r + 1) * height) / grid.rows)));
    for (let c = 0; c < grid.cols; c++) {
      const x0 = Math.min(width - 1, Math.floor((c * width) / grid.cols));
      const x1 = Math.min(width, Math.max(x0 + 1, Math.floor(((c + 1) * width) / grid.cols)));
      let sum = 0;
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const i = 4 * (y * width + x);
          sum += 0.2126 * rgba[i]! + 0.7152 * rgba[i + 1]! + 0.0722 * rgba[i + 2]!;
        }
      }
      out[r * grid.cols + c] = sum / (255 * (y1 - y0) * (x1 - x0));
    }
  }
  return out;
}

/**
 * Frame differences, like an event camera: 2 × cells channels, the cells' "on" channels (brightness rose by more than
 * `level` since the last frame) then their "off" channels (it fell). A channel carries the size of the change when
 * it counts, else 0, so any sample encoding works.
 */
export function frameChanges(previous: ArrayLike<number>, current: ArrayLike<number>, level: number): Float32Array {
  const cells = current.length;
  const out = new Float32Array(2 * cells);
  for (let i = 0; i < cells; i++) {
    const d = current[i]! - previous[i]!;
    if (d > level) out[i] = d;
    else if (-d > level) out[cells + i] = -d;
  }
  return out;
}

export const cellsOf = (grid: Grid) => grid.cols * grid.rows;

/** How many channels a camera sends: one per cell, or two (on, off) for frame differences. */
export const cameraChannels = (grid: Grid, mode: CameraMode) => cellsOf(grid) * (mode === "changes" ? 2 : 1);

/** The channels a source shows in a preview with no network to say how many neurons it drives. */
export function previewChannels(s: RealtimeSource): number {
  switch (s.kind) {
    case "microphone":
      return s.bands ?? PREVIEW_BANDS;
    case "camera":
      return cameraChannels(s.grid ?? DEFAULT_GRID, s.mode);
    case "sensor":
      return 1;
  }
}

// ----- speed -----

/** "×0.5", "×2", "max". */
export const speedText = (speed: number | null): string => (speed === null ? "max" : `×${Number(speed.toPrecision(3))}`);

/**
 * Whether a realtime source may stream at the run's `speed` (simulated seconds per wall second; null: as fast as
 * possible). Realtime data can't be had faster than it happens, so a run as fast as possible takes none; a source with
 * `speed = "refuse"` runs only at ×1; at any other speed the engine scales the source's time by it ("resample").
 */
export function speedVerdict(s: RealtimeSource, speed: number | null): { ok: true } | { ok: false; message: string } {
  if (speed === null) return { ok: false, message: "the run is as fast as possible: realtime input needs a set speed" };
  if (s.speed === "refuse" && speed !== 1) return { ok: false, message: `the run is at speed ${speedText(speed)} and this source runs only at ×1` };
  return { ok: true };
}
