// Live data: frames from the engine → stream-charts data sources and timing statistics. As in stream-charts'
// examples, the data sources live in stores (here, `useLive` and `useInspect`), not in components: they keep
// ingesting while charts unmount and remount, and they outlive changes of what's shown. Changing the raster's
// neurons, or the inspected ones, gives the chart a new data source seeded with the data the old one had (and, for
// neurons new to the raster, the spikes recorded from the whole-network stream), so nothing already shown is lost.
// A session's views last until the session or its build changes.

import { type Frame, Lod, type ProtocolClient, spikeCount, timeMs, type View, type Viewer } from "@spikes/client";
import { BehaviorSubject, map, type Observable, Subject } from "rxjs";
import {
  type Datum,
  emptySeries,
  seriesFrom,
  type TimeSeries,
  type TimeSeriesChartData,
  TimeSeriesDataSource,
} from "stream-charts";
import { create } from "zustand";
import { RunGate } from "./runGate";
import { chartSettings, useChartSettings } from "./chartSettings";
import { useSession } from "./store";

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

export interface Neuron {
  index: number;
  name: string;
}

export function chartData(names: string[], time: number, points: Map<string, Datum[]>): TimeSeriesChartData {
  return {
    seriesNames: new Set(names),
    maxTime: time,
    maxTimes: new Map(names.map((n) => [n, time])),
    newPoints: points,
  };
}

const sameNeurons = (a: Neuron[], b: Neuron[]) => a.length === b.length && a.every((n, i) => n.index === b[i]!.index && n.name === b[i]!.name);

/** A time-series data source fed by `frames`, running while `gate` lets it (see {@link RunGate}). */
function source(initialData: TimeSeries[], frames: Observable<Frame>, convert: (frame: Frame) => TimeSeriesChartData, gate: RunGate): TimeSeriesDataSource {
  return gate.add(
    new TimeSeriesDataSource({
      initialData,
      generator: () => frames.pipe(map(convert)),
      dropDataAfter: chartSettings().keepMs,
    }),
  );
}

/** Series named `names`, with the data `old` had for them (copied), or what `fill` gives. */
function carried(old: TimeSeriesDataSource, names: string[], fill: (name: string, i: number) => Datum[] = () => []): TimeSeries[] {
  const had = new Map(old.seriesList().map((s) => [s.name, s.data]));
  return names.map((name, i) => seriesFrom<Datum>(name, had.get(name)?.slice() ?? fill(name, i)));
}

/**
 * Recent spike times of every neuron, from the whole-network stream (when it sends exact spikes): what a neuron
 * newly added to the raster did before. Bounded: beyond a few million spikes it gives up (big networks stream
 * aggregates anyway).
 */
class SpikeHistory {
  private readonly times = new Map<number, number[]>();
  private total = 0;
  private enabled = true;
  private static readonly LIMIT = 3_000_000;

  record(frame: Frame) {
    const s = frame.spikes;
    if (!this.enabled || !s) return;
    for (let i = 0; i < s.neuron.length; i++) {
      const n = s.neuron[i]!;
      let list = this.times.get(n);
      if (!list) this.times.set(n, (list = []));
      list.push(timeMs(frame.header, s.offset[i]!));
    }
    this.total += s.neuron.length;
    if (this.total > SpikeHistory.LIMIT) this.trim(frame.header.t1Ms - chartSettings().keepMs);
  }

  private trim(from: number) {
    this.total = 0;
    for (const [n, list] of this.times) {
      const kept = list.filter((t) => t >= from);
      if (kept.length) this.times.set(n, kept);
      else this.times.delete(n);
      this.total += kept.length;
    }
    if (this.total > SpikeHistory.LIMIT / 2) {
      this.times.clear();
      this.enabled = false;
    }
  }

  since(neuron: number, from: number): number[] {
    return (this.times.get(neuron) ?? []).filter((t) => t >= from);
  }
}

/**
 * A session's live view: the whole network (for the population rate, the 3D view and timing) and the raster's
 * neurons (see {@link LiveView.select}).
 */
export class LiveView {
  readonly rate: TimeSeriesDataSource;
  /** The raster's data source: a new one whenever its neurons change. */
  readonly raster: BehaviorSubject<TimeSeriesDataSource>;
  readonly stats = new BehaviorSubject<LiveStats>(EMPTY_STATS);
  private readonly whole = new Subject<Frame>();
  /** The whole-network frames, through the gate (for the rate). */
  private readonly rateFrames = new Subject<Frame>();
  private readonly selected = new Subject<Frame>();
  /** The whole-network frames (for the 3D view). */
  readonly frames: Observable<Frame> = this.whole.asObservable();
  private wholeViewer: Viewer | null = null;
  private rasterViewer: Viewer | null = null;
  private selection: Neuron[] = [];
  private selecting = 0;
  private readonly history = new SpikeHistory();
  private readonly frameTimes: number[] = [];
  private readonly spikeTimes: Array<[number, number]> = [];
  private lastRateMs = -1;
  private readonly unfollow: () => void;
  private readonly gate = new RunGate();
  private disposed = false;

  private constructor(
    private readonly client: ProtocolClient,
    private readonly session: number,
    private readonly neurons: number,
  ) {
    this.rate = source([emptySeries<Datum>("rate")], this.rateFrames, (f) => this.rateData(f), this.gate);
    this.raster = new BehaviorSubject(this.rasterSource([], []));
    // Settings apply as they change: retention to the data sources, the update period to the raster's frames.
    this.unfollow = useChartSettings.subscribe((s, before) => {
      if (s.keepMs !== before.keepMs) {
        this.rate.setDropDataAfter(s.keepMs);
        this.raster.value.setDropDataAfter(s.keepMs);
      }
      if (s.updateMs !== before.updateMs) void this.rasterViewer?.update(this.rasterView()).catch(() => {});
    });
  }

  static async attach(client: ProtocolClient, session: number, neurons: number): Promise<LiveView> {
    const view = new LiveView(client, session, neurons);
    // The whole network at 16 ms, for the 3D view's flashes (and the rate, and the spike history).
    view.wholeViewer = await client.attach(
      session,
      { frameMs: 16, budgetBytesPerS: 1 << 20, spikes: { type: "all" }, membrane: [], weights: { type: "nothing" } },
      (f) => view.onWhole(f),
    );
    return view;
  }

  private rasterView(): View {
    return {
      frameMs: chartSettings().updateMs,
      budgetBytesPerS: 4 << 20,
      spikes: { type: "neurons", neurons: this.selection.map((n) => n.index) },
      membrane: [],
      weights: { type: "nothing" },
    };
  }

  private rasterSource(series: TimeSeries[], neurons: Neuron[]): TimeSeriesDataSource {
    const names = new Map(neurons.map((n) => [n.index, n.name]));
    return source(series, this.selected, (f) => rasterData(f, names), this.gate);
  }

  /** Shows these neurons in the raster, keeping what it showed of the ones it already had. */
  async select(neurons: Neuron[]): Promise<void> {
    if (this.disposed || sameNeurons(neurons, this.selection)) return;
    const turn = ++this.selecting;
    this.selection = neurons;
    // The engine sends the new neurons' spikes from now on...
    if (this.rasterViewer) await this.rasterViewer.update(this.rasterView());
    else
      this.rasterViewer = await this.client.attach(this.session, this.rasterView(), (f) =>
        this.gate.deliver(f.header.t1Ms, () => this.selected.next(f)),
      );
    if (this.disposed || turn !== this.selecting) return;
    // ...and their past is what the raster had, or (for neurons new to it) the whole-network stream's.
    const old = this.raster.value;
    const from = this.stats.value.simMs - chartSettings().keepMs;
    const series = carried(
      old,
      neurons.map((n) => n.name),
      (_, i) => this.history.since(neurons[i]!.index, from).map((t) => ({ x: t, y: neurons[i]!.index })),
    );
    this.raster.next(this.rasterSource(series, neurons));
    this.gate.remove(old);
    old.dispose();
  }

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
    this.gate.deliver(frame.header.t1Ms, () => this.rateFrames.next(frame));
    this.history.record(frame);
    this.whole.next(frame);
    const now = performance.now();
    this.frameTimes.push(now);
    this.spikeTimes.push([now, spikeCount(frame)]);
    while (this.frameTimes.length && this.frameTimes[0]! < now - 1000) this.frameTimes.shift();
    while (this.spikeTimes.length && this.spikeTimes[0]![0] < now - 1000) this.spikeTimes.shift();
    const h = frame.header;
    // Spikes per wall second over the last second of frames.
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
    this.unfollow();
    this.gate.dispose();
    await Promise.allSettled([this.wholeViewer, this.rasterViewer].map((v) => v?.detach()));
    this.raster.value.dispose();
    this.raster.complete();
    this.rate.dispose();
    this.stats.complete();
  }
}

function rasterData(frame: Frame, names: Map<number, string>): TimeSeriesChartData {
  const points = new Map<string, Datum[]>();
  const add = (neuron: number, t: number) => {
    const name = names.get(neuron);
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
  return chartData([...names.values()], frame.header.t1Ms, points);
}

/** A synapse shown in the weights chart. */
export interface WeightSeries {
  post: number;
  synapse: number;
  name: string;
  weight: number;
}

/**
 * The inspector's live view: membrane potential (on every update) of a few neurons, and the weights of their
 * incoming synapses (see {@link InspectView.follow}). Its own viewer asks only for those, so it stays cheap at any
 * network size.
 */
export class InspectView {
  readonly membrane: BehaviorSubject<TimeSeriesDataSource>;
  readonly weights: BehaviorSubject<TimeSeriesDataSource>;
  private readonly frames = new Subject<Frame>();
  private viewer: Viewer | null = null;
  private neurons: Neuron[] = [];
  private synapses: WeightSeries[] = [];
  /** Each weight series' latest value (the lines run on between changes). */
  private latest = new Map<string, number>();
  private following = 0;
  private readonly unfollow: () => void;
  private readonly gate = new RunGate();
  private disposed = false;

  constructor(
    private readonly client: ProtocolClient,
    private readonly session: number,
  ) {
    this.membrane = new BehaviorSubject(this.membraneSource([], []));
    this.weights = new BehaviorSubject(this.weightSource([], []));
    this.unfollow = useChartSettings.subscribe((s, before) => {
      if (s.keepMs !== before.keepMs) {
        this.membrane.value.setDropDataAfter(s.keepMs);
        this.weights.value.setDropDataAfter(s.keepMs);
      }
      if (s.updateMs !== before.updateMs) void this.viewer?.update(this.view()).catch(() => {});
    });
  }

  private view(): View {
    const indices = this.neurons.map((n) => n.index);
    return {
      // Membrane traces at 16 ms (they're drawn as lines); the weights change rarely.
      frameMs: Math.min(16, chartSettings().updateMs),
      budgetBytesPerS: 4 << 20,
      spikes: { type: "nothing" },
      membrane: indices,
      weights: { type: "neurons", neurons: indices },
    };
  }

  private membraneSource(series: TimeSeries[], neurons: Neuron[]): TimeSeriesDataSource {
    const names = new Map(neurons.map((n) => [n.index, n.name]));
    return source(series, this.frames, (f) => membraneData(f, names), this.gate);
  }

  private weightSource(series: TimeSeries[], synapses: WeightSeries[]): TimeSeriesDataSource {
    const byKey = new Map(synapses.map((s) => [`${s.post}:${s.synapse}`, s]));
    return source(series, this.frames, (f) => this.weightData(f, byKey), this.gate);
  }

  /** Follows these neurons and synapses, keeping what was shown of the ones already followed. */
  async follow(neurons: Neuron[], synapses: WeightSeries[]): Promise<void> {
    const sameSynapses =
      synapses.length === this.synapses.length && synapses.every((s, i) => s.name === this.synapses[i]!.name && s.post === this.synapses[i]!.post);
    if (this.disposed || (sameNeurons(neurons, this.neurons) && sameSynapses)) return;
    const turn = ++this.following;
    this.neurons = neurons;
    this.synapses = synapses;
    if (this.viewer) await this.viewer.update(this.view());
    else
      this.viewer = await this.client.attach(this.session, this.view(), (f) => this.gate.deliver(f.header.t1Ms, () => this.frames.next(f)));
    if (this.disposed || turn !== this.following) return;
    this.latest = new Map(synapses.map((s) => [s.name, this.latest.get(s.name) ?? s.weight]));
    const [oldMembrane, oldWeights] = [this.membrane.value, this.weights.value];
    this.membrane.next(this.membraneSource(carried(oldMembrane, neurons.map((n) => n.name)), neurons));
    this.weights.next(this.weightSource(carried(oldWeights, synapses.map((s) => s.name)), synapses));
    this.gate.remove(oldMembrane);
    this.gate.remove(oldWeights);
    oldMembrane.dispose();
    oldWeights.dispose();
  }

  private weightData(frame: Frame, synapses: Map<string, WeightSeries>): TimeSeriesChartData {
    const w = frame.weights;
    if (w) {
      w.post.forEach((post, i) => {
        const series = synapses.get(`${post}:${w.synapse[i]}`);
        if (series) this.latest.set(series.name, w.weight[i]!);
      });
    }
    // Every series gets its current value each frame, so the lines run on between changes.
    const t = frame.header.t1Ms;
    const names = [...synapses.values()].map((s) => s.name);
    const points = new Map<string, Datum[]>(names.map((name) => [name, [{ x: t, y: this.latest.get(name) ?? 0 }]]));
    return chartData(names, t, points);
  }

  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    this.unfollow();
    this.gate.dispose();
    await this.viewer?.detach().catch(() => {});
    for (const s of [this.membrane, this.weights]) {
      s.value.dispose();
      s.complete();
    }
  }
}

function membraneData(frame: Frame, names: Map<number, string>): TimeSeriesChartData {
  const points = new Map<string, Datum[]>();
  const m = frame.membrane;
  if (m) {
    m.neuron.forEach((n, i) => {
      const name = names.get(n);
      if (name === undefined) return;
      let list = points.get(name);
      if (!list) points.set(name, (list = []));
      list.push({ x: timeMs(frame.header, m.offset[i]!), y: m.mv[i]! });
    });
  }
  return chartData([...names.values()], frame.header.t1Ms, points);
}

// ----- the stores that hold a session's views -----

/** A store holding one view for the current session and build: `connect` makes it (once per session and build),
 *  `disconnect` disposes of it. */
interface ViewStore<V> {
  view: V | null;
  key: string | null;
  client: ProtocolClient | null;
  error: string | null;
  connect: (client: ProtocolClient, session: number, build: number, make: () => Promise<V>) => void;
  disconnect: () => void;
}

export function viewStore<V extends { dispose: () => Promise<void> }>() {
  return create<ViewStore<V>>()((set, get) => ({
    view: null,
    key: null,
    client: null,
    error: null,
    connect: (client, session, build, make) => {
      const key = `${session}:${build}`;
      const current = get();
      if (current.key === key && current.client === client) return;
      current.disconnect();
      set({ key, client, error: null });
      make()
        .then((view) => {
          if (get().key !== key) return void view.dispose();
          set({ view });
        })
        .catch((e: Error) => set({ error: e.message }));
    },
    disconnect: () => {
      void get().view?.dispose();
      set({ view: null, key: null, client: null });
    },
  }));
}

/** The Run page's live view (raster, rate, 3D frames, timing). */
export const useLive = viewStore<LiveView>();
/** The inspector's live view. */
export const useInspect = viewStore<InspectView>();

export function connectLive(client: ProtocolClient, session: number, build: number, neurons: number) {
  useLive.getState().connect(client, session, build, () => LiveView.attach(client, session, neurons));
}

export function connectInspect(client: ProtocolClient, session: number, build: number) {
  useInspect.getState().connect(client, session, build, async () => new InspectView(client, session));
}

/** A view belongs to a session and build: when either changes, its store lets it go (the page connects anew). */
export function releaseOnSessionChange(store: { getState: () => { disconnect: () => void } }) {
  useSession.subscribe((s, before) => {
    if (s.id !== before.id || s.build !== before.build) store.getState().disconnect();
  });
}

releaseOnSessionChange(useLive);
releaseOnSessionChange(useInspect);

// For tests and debugging in the browser console: `__spikes.live.getState().view`.
if (typeof window !== "undefined") Object.assign(window, { __spikes: { live: useLive, inspect: useInspect, charts: useChartSettings } });
