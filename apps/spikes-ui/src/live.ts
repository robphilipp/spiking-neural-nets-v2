// Live data: frames from the engine → stream-charts data sources and timing statistics. Nothing here goes through
// Redux; charts draw from the data sources directly, outside React's render cycle.

import { type Frame, Lod, type ProtocolClient, spikeCount, timeMs, type Viewer } from "@spikes/client";
import { BehaviorSubject, map, type Observable, Subject } from "rxjs";
import {
  type DataGenerator,
  type Datum,
  emptySeries,
  type TimeSeries,
  type TimeSeriesChartData,
  TimeSeriesDataSource,
} from "stream-charts";

/** What the timing-health panel shows. */
export interface LiveStats {
  simMs: number;
  /** Simulated seconds per wall second; Infinity as fast as possible; 0 paused. */
  speed: number;
  lagMs: number;
  framesPerS: number;
  spikesPerS: number;
  lod: Lod;
  droppedFrames: number;
  droppedEvents: number;
}

const EMPTY_STATS: LiveStats = {
  simMs: 0,
  speed: 0,
  lagMs: 0,
  framesPerS: 0,
  spikesPerS: 0,
  lod: Lod.Exact,
  droppedFrames: 0,
  droppedEvents: 0,
};

/** How much simulated time the charts keep, ms. */
const KEEP_MS = 10_000;

function chartData(names: string[], time: number, points: Map<string, Datum[]>): TimeSeriesChartData {
  return {
    seriesNames: new Set(names),
    maxTime: time,
    maxTimes: new Map(names.map((n) => [n, time])),
    newPoints: points,
  };
}

/** Two live views of a session: the whole network (for rates and timing) and a selection (for the raster). */
export class LiveView {
  readonly raster: TimeSeriesDataSource;
  readonly rate: TimeSeriesDataSource;
  readonly stats = new BehaviorSubject<LiveStats>(EMPTY_STATS);
  private readonly selected = new Subject<Frame>();
  private readonly whole = new Subject<Frame>();
  private readonly viewers: Viewer[] = [];
  private readonly frameTimes: number[] = [];
  private readonly spikeTimes: Array<[number, number]> = [];
  private disposed = false;

  /**
   * @param names the selected neurons' names (raster rows), by neuron index
   * @param neurons the total number of neurons
   */
  private constructor(
    private readonly names: Map<number, string>,
    private readonly neurons: number,
  ) {
    const rows = [...names.values()];
    this.raster = new TimeSeriesDataSource({
      initialData: rows.map((n) => emptySeries<Datum>(n)),
      generator: this.generator(this.selected, (frame) => this.rasterData(frame)),
      dropDataAfter: KEEP_MS,
    });
    this.rate = new TimeSeriesDataSource({
      initialData: [emptySeries<Datum>("rate")],
      generator: this.generator(this.whole, (frame) => this.rateData(frame)),
      dropDataAfter: KEEP_MS,
    });
  }

  /** Attaches the two views; frames start flowing into the data sources. */
  static async attach(
    client: ProtocolClient,
    session: number,
    selection: Array<{ index: number; name: string }>,
    neurons: number,
  ): Promise<LiveView> {
    const view = new LiveView(new Map(selection.map((n) => [n.index, n.name])), neurons);
    view.raster.start();
    view.rate.start();
    view.viewers.push(
      await client.attach(
        session,
        { frameMs: 16, budgetBytesPerS: 1 << 20, spikes: { type: "all" }, membrane: [], weights: { type: "nothing" } },
        (f) => view.onWhole(f),
      ),
      await client.attach(
        session,
        {
          frameMs: 16,
          budgetBytesPerS: 4 << 20,
          spikes: { type: "neurons", neurons: selection.map((n) => n.index) },
          membrane: [],
          weights: { type: "nothing" },
        },
        (f) => view.selected.next(f),
      ),
    );
    return view;
  }

  private generator(
    frames: Observable<Frame>,
    convert: (frame: Frame) => TimeSeriesChartData,
  ): DataGenerator<TimeSeriesChartData, TimeSeries> {
    return () => frames.pipe(map(convert));
  }

  private rasterData(frame: Frame): TimeSeriesChartData {
    const points = new Map<string, Datum[]>();
    const add = (neuron: number, t: number) => {
      const name = this.names.get(neuron);
      if (name === undefined) return;
      let list = points.get(name);
      if (!list) points.set(name, (list = []));
      list.push({ x: t, y: neuron });
    };
    if (frame.spikes) {
      frame.spikes.neuron.forEach((n, i) => add(n, timeMs(frame.header, frame.spikes!.offset[i]!)));
    } else if (frame.spikeCounts) {
      // Too many for exact times: one mark per neuron, mid-frame.
      const mid = (frame.header.t0Ms + frame.header.t1Ms) / 2;
      frame.spikeCounts.neuron.forEach((n) => add(n, mid));
    }
    return chartData([...this.names.values()], frame.header.t1Ms, points);
  }

  private lastRateMs = -1;

  private rateData(frame: Frame): TimeSeriesChartData {
    const { t0Ms, t1Ms } = frame.header;
    const points = new Map<string, Datum[]>();
    const span = t1Ms - Math.max(t0Ms, this.lastRateMs);
    if (span > 0 && this.neurons > 0) {
      points.set("rate", [{ x: t1Ms, y: (spikeCount(frame) / this.neurons) * (1000 / span) }]);
      this.lastRateMs = t1Ms;
    }
    return chartData(["rate"], t1Ms, points);
  }

  private onWhole(frame: Frame) {
    this.whole.next(frame);
    const now = performance.now();
    this.frameTimes.push(now);
    this.spikeTimes.push([now, spikeCount(frame)]);
    while (this.frameTimes.length && this.frameTimes[0]! < now - 1000) this.frameTimes.shift();
    while (this.spikeTimes.length && this.spikeTimes[0]![0] < now - 1000) this.spikeTimes.shift();
    const h = frame.header;
    // Spikes per wall second over the last second of frames, scaled to simulated seconds by the speed.
    const spikes = this.spikeTimes.reduce((n, [, c]) => n + c, 0);
    this.stats.next({
      simMs: h.t1Ms,
      speed: h.speed,
      lagMs: h.lagMs,
      framesPerS: this.frameTimes.length,
      spikesPerS: spikes,
      lod: h.lod,
      droppedFrames: h.droppedFrames,
      droppedEvents: h.droppedEvents,
    });
  }

  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    await Promise.allSettled(this.viewers.map((v) => v.detach()));
    this.raster.dispose();
    this.rate.dispose();
    this.stats.complete();
  }
}
