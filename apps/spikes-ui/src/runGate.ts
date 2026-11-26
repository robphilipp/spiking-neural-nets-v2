// Chart data sources that run only while the simulation does, as in stream-charts' examples. A running source is
// what keeps a chart's time axis moving (with cadence, between data arrivals too), so a source left running after
// the simulation stops or pauses would scroll its chart past the data.
//
// The engine sends a frame only when there's something new, so frames stop when the simulation does. A source runs
// while the session runs, and for a moment after a frame arrives (the run's last frames, which can come just after
// it stops; a step while paused; the views' first summaries); then it stops, keeping its data, and its chart stays
// where the data ends (zoom and all). Each frame starts the sources (if stopped) before it's handed on, so none is
// lost.
//
// Frozen charts (see the chart settings) hold still while the run goes on: their sources stop, and what arrives is
// kept (back to the charts' retention) and handed on when they unfreeze, so they catch up and carry on.

import { chartSettings, useChartSettings } from "./chartSettings";
import { useSession } from "./store";

/** What the gate starts and stops (a stream-charts data source). */
export interface Startable {
  start(): void;
  stop(): void;
}

/** The shortest quiet after the last frame before the sources stop, ms. */
const MIN_QUIET_MS = 150;

/** Something that arrived while frozen: its simulated time, and how to hand it on. */
interface Held {
  t: number;
  deliver: () => void;
}

export class RunGate {
  private readonly sources = new Set<Startable>();
  /** Whether the sources run. */
  private started = false;
  private lastFrame = -Infinity;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private held: Held[] = [];
  private readonly unfollow: Array<() => void>;

  constructor() {
    this.unfollow = [
      useSession.subscribe((s, before) => {
        if (s.state !== before.state) this.update();
      }),
      useChartSettings.subscribe((s, before) => {
        if (s.frozen !== before.frozen) {
          if (s.frozen) this.update();
          else this.release();
        }
      }),
    ];
    this.update();
  }

  /** Starts and stops `source` with the others from now on (starting it now if they run). */
  add<S extends Startable>(source: S): S {
    this.sources.add(source);
    if (this.started) source.start();
    return source;
  }

  /** No longer starts or stops `source` (call before disposing of it). */
  remove(source: Startable) {
    this.sources.delete(source);
  }

  /**
   * Hands on something that arrived (a frame, at simulated time `t`): now, with the sources running (they run until
   * frames have stopped for a moment); or, while the charts are frozen, when they unfreeze.
   */
  deliver(t: number, deliver: () => void) {
    if (useChartSettings.getState().frozen) {
      this.held.push({ t, deliver });
      // Kept back to the charts' retention: older data would be dropped on arrival anyway.
      const from = t - chartSettings().keepMs;
      let old = 0;
      while (old < this.held.length && this.held[old]!.t < from) old++;
      if (old > 0) this.held.splice(0, old);
      return;
    }
    this.lastFrame = performance.now();
    this.update();
    deliver();
  }

  /** Unfrozen: the sources take in what arrived meanwhile, and carry on. */
  private release() {
    const held = this.held;
    this.held = [];
    this.lastFrame = performance.now();
    this.update();
    for (const h of held) h.deliver();
  }

  private quietMs(): number {
    return Math.max(MIN_QUIET_MS, 2 * chartSettings().updateMs + 50);
  }

  private update() {
    clearTimeout(this.timer);
    const running = useSession.getState().state === "running";
    const sinceFrame = performance.now() - this.lastFrame;
    const quiet = this.quietMs();
    const active = running || sinceFrame < quiet;
    const start = active && !useChartSettings.getState().frozen;
    if (start !== this.started) {
      this.started = start;
      for (const source of this.sources) {
        if (start) source.start();
        else source.stop();
      }
    }
    // Not running, but frames came just now: look again once they've been quiet long enough.
    if (start && !running) this.timer = setTimeout(() => this.update(), quiet - sinceFrame + 1);
  }

  dispose() {
    clearTimeout(this.timer);
    for (const unfollow of this.unfollow) unfollow();
    this.sources.clear();
    this.held = [];
  }
}
