// Chart data sources that run only while the simulation does, as in stream-charts' examples. A running source is
// what keeps a chart's time axis moving (with cadence, between data arrivals too), so a source left running after
// the simulation stops or pauses would scroll its chart past the data.
//
// The engine sends a frame only when there's something new, so frames stop when the simulation does. A source runs
// while the session runs, and for a moment after a frame arrives (the run's last frames, which can come just after
// it stops; a step while paused; the views' first summaries); then it stops, keeping its data, and its chart stays
// where the data ends (zoom and all). Each frame starts the sources (if stopped) before it's handed on, so none is
// lost.

import { chartSettings } from "./chartSettings";
import { useSession } from "./store";

/** What the gate starts and stops (a stream-charts data source). */
export interface Startable {
  start(): void;
  stop(): void;
}

/** The shortest quiet after the last frame before the sources stop, ms. */
const MIN_QUIET_MS = 150;

export class RunGate {
  private readonly sources = new Set<Startable>();
  private active = false;
  private lastFrame = -Infinity;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private readonly unfollow: () => void;

  constructor() {
    this.unfollow = useSession.subscribe((s, before) => {
      if (s.state !== before.state) this.update();
    });
    this.update();
  }

  /** Starts and stops `source` with the others from now on (starting it now if they run). */
  add<S extends Startable>(source: S): S {
    this.sources.add(source);
    if (this.active) source.start();
    return source;
  }

  /** No longer starts or stops `source` (call before disposing of it). */
  remove(source: Startable) {
    this.sources.delete(source);
  }

  /** A frame arrived: the sources run (now, before it's handed on) until frames have stopped for a moment. */
  arrived() {
    this.lastFrame = performance.now();
    this.update();
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
    if (active !== this.active) {
      this.active = active;
      for (const source of this.sources) {
        if (active) source.start();
        else source.stop();
      }
    }
    // Not running, but frames came just now: look again once they've been quiet long enough.
    if (active && !running) this.timer = setTimeout(() => this.update(), quiet - sinceFrame + 1);
  }

  dispose() {
    clearTimeout(this.timer);
    this.unfollow();
    this.sources.clear();
  }
}
