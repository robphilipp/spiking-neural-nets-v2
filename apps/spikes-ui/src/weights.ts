// The network's synapse weights, live: every weight change from the engine, summarized as a distribution over time
// (mean and percentiles) and as a histogram of the current weights.

import type { Network, ProtocolClient, Viewer } from "@spikes/client";
import { BehaviorSubject, map, Subject } from "rxjs";
import { type Datum, emptySeries, TimeSeriesDataSource } from "stream-charts";
import { chartSettings, useChartSettings } from "./chartSettings";
import { chartData, releaseOnSessionChange, viewStore } from "./live";
import { RunGate } from "./runGate";

/** The series of the weight trend chart. */
export const WEIGHT_SERIES = ["mean", "median", "10th percentile", "90th percentile"] as const;

export interface WeightSummary {
  /** Simulated time, ms. */
  t: number;
  /** Synapses known (all of them when the network lists its synapses; otherwise only the ones that changed). */
  count: number;
  /** Synapses whose weight has changed since this view started. */
  changed: number;
  mean: number;
  p10: number;
  p50: number;
  p90: number;
  /** A histogram of the current weights: `counts[i]` in `[lo + i·width, lo + (i+1)·width)`, over the 0.5th–99.5th
   *  percentiles (the end bins include the outliers beyond). */
  lo: number;
  width: number;
  counts: number[];
}

const BINS = 40;

const key = (post: number, synapse: number) => post * 4_294_967_296 + synapse;

export class WeightsView {
  readonly trend: TimeSeriesDataSource;
  readonly summary = new BehaviorSubject<WeightSummary | null>(null);
  private readonly points = new Subject<{ t: number; s: WeightSummary }>();
  private readonly weights = new Map<number, number>();
  private readonly changed = new Set<number>();
  private viewer: Viewer | null = null;
  private lastWall = 0;
  private lastT = 0;
  private dirty = true;
  private disposed = false;
  private readonly gate = new RunGate();
  private readonly unfollow: () => void;

  private constructor(network: Network) {
    // A network that lists its synapses gives every starting weight; a big one doesn't, so only changes are known.
    if (network.synapses.length === network.synapseCount) {
      for (const s of network.synapses) this.weights.set(key(s.post, s.synapse), s.weight);
    }
    this.trend = this.gate.add(new TimeSeriesDataSource({
      initialData: WEIGHT_SERIES.map((n) => emptySeries<Datum>(n)),
      generator: () =>
        this.points.pipe(
          map(({ t, s }) =>
            chartData(
              [...WEIGHT_SERIES],
              t,
              new Map<string, Datum[]>([
                ["mean", [{ x: t, y: s.mean }]],
                ["median", [{ x: t, y: s.p50 }]],
                ["10th percentile", [{ x: t, y: s.p10 }]],
                ["90th percentile", [{ x: t, y: s.p90 }]],
              ]),
            ),
          ),
        ),
      dropDataAfter: chartSettings().keepMs,
    }));
    this.unfollow = useChartSettings.subscribe((s, before) => {
      if (s.keepMs !== before.keepMs) this.trend.setDropDataAfter(s.keepMs);
      if (s.updateMs !== before.updateMs) void this.viewer?.update(WeightsView.view()).catch(() => {});
    });
  }

  private static view() {
    return {
      frameMs: chartSettings().updateMs,
      budgetBytesPerS: 8 << 20,
      spikes: { type: "nothing" as const },
      membrane: [],
      weights: { type: "all" as const },
    };
  }

  static async attach(client: ProtocolClient, session: number, network: Network): Promise<WeightsView> {
    const view = new WeightsView(network);
    view.gate.arrived();
    view.publish(0, true);
    view.viewer = await client.attach(
      session,
      WeightsView.view(),
      (f) => {
        view.gate.arrived();
        const w = f.weights;
        if (w) {
          for (let i = 0; i < w.post.length; i++) {
            const k = key(w.post[i]!, w.synapse[i]!);
            view.weights.set(k, w.weight[i]!);
            view.changed.add(k);
          }
          view.dirty ||= w.post.length > 0;
        }
        view.publish(f.header.t1Ms, false);
      },
    );
    return view;
  }

  /** Summarizes the current weights, at most once per update period of wall time (or now, if forced): simulated
   *  time can run much faster than wall time. */
  private publish(t: number, force: boolean) {
    const now = performance.now();
    if (!force && now - this.lastWall < chartSettings().updateMs) return;
    this.lastWall = now;
    // Time can go back (a rebuilt network): restart the trend.
    if (t < this.lastT) this.dirty = true;
    this.lastT = t;
    const s = this.dirty || !this.summary.value ? this.summarize(t) : { ...this.summary.value, t };
    this.dirty = false;
    this.summary.next(s);
    if (s.count > 0) this.points.next({ t, s });
  }

  private summarize(t: number): WeightSummary {
    let lo = Infinity;
    let hi = -Infinity;
    let sum = 0;
    for (const w of this.weights.values()) {
      if (w < lo) lo = w;
      if (w > hi) hi = w;
      sum += w;
    }
    const count = this.weights.size;
    if (!count) return { t, count, changed: 0, mean: 0, p10: 0, p50: 0, p90: 0, lo: 0, width: 1, counts: [] };
    // Pad an all-equal distribution so it gets a visible bin.
    if (hi - lo < 1e-9) {
      lo -= 0.5;
      hi += 0.5;
    }
    // The histogram spans the 0.5th–99.5th percentiles (found on a fine histogram first), so a few outliers don't
    // squash the rest into a couple of bins; outliers are counted in the end bins.
    const FINE = 2000;
    const fineWidth = (hi - lo) / FINE;
    const fine = new Uint32Array(FINE);
    for (const w of this.weights.values()) fine[Math.min(FINE - 1, Math.floor((w - lo) / fineWidth))]! += 1;
    const fineQuantile = (q: number) => {
      const target = q * count;
      let seen = 0;
      for (let i = 0; i < FINE; i++) {
        seen += fine[i]!;
        if (seen >= target) return lo + fineWidth * (q < 0.5 ? i : i + 1);
      }
      return hi;
    };
    const percentiles = (q: number) => {
      const target = q * count;
      let seen = 0;
      for (let i = 0; i < FINE; i++) {
        const c = fine[i]!;
        if (seen + c >= target && c > 0) return lo + fineWidth * (i + (target - seen) / c);
        seen += c;
      }
      return hi;
    };
    let from = fineQuantile(0.005);
    let to = fineQuantile(0.995);
    if (to - from < 1e-9) {
      from = lo;
      to = hi;
    }
    const width = (to - from) / BINS;
    const counts = new Array<number>(BINS).fill(0);
    for (const w of this.weights.values()) counts[Math.max(0, Math.min(BINS - 1, Math.floor((w - from) / width)))]! += 1;
    return {
      t,
      count,
      changed: this.changed.size,
      mean: sum / count,
      p10: percentiles(0.1),
      p50: percentiles(0.5),
      p90: percentiles(0.9),
      lo: from,
      width,
      counts,
    };
  }
  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    this.unfollow();
    this.gate.dispose();
    await this.viewer?.detach().catch(() => {});
    this.trend.dispose();
  }
}

/** The weights panel's view of the current session and build. */
export const useWeights = viewStore<WeightsView>();
releaseOnSessionChange(useWeights);

export function connectWeights(client: ProtocolClient, session: number, build: number, network: Network) {
  useWeights.getState().connect(client, session, build, () => WeightsView.attach(client, session, network));
}
