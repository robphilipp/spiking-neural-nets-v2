// The realtime sources of a run (app plan 05, phase I4): every enabled microphone, camera and sensor source of the
// input file streams into the running session as a sensor named after the source, each with its own status. Several
// at once (decision 9). The manager lives outside any page (see realtime.ts), so streams keep going while you look at
// other pages; it follows the run: it starts the sources when the run is running, stops them when it ends or the
// session changes, and holds them across pause and resume (the sensors' `on_pause` is the engine's: it drops or
// buffers what arrives while paused). No DOM in here: the devices come in as `open`, so the rules are tested.

import type { Encoding, Network, SensorInfo, SensorSpec, Sensor } from "@spikes/client";
import { type InputFile, InputFileError, isRealtime, type RealtimeSource, targetNeurons } from "./inputFile";
import type { Capture, OpenCapture } from "./capture";
import { channelPlan, speedVerdict } from "./realtimeChannels";

export type RealtimeState =
  /** Opening the device, or connected and waiting for the first samples. */
  | "starting"
  | "streaming"
  | "stopped"
  /** Not started: the run's speed doesn't allow it. */
  | "refused"
  | "error";

export interface RealtimeStatus {
  name: string;
  kind: RealtimeSource["kind"];
  state: RealtimeState;
  /** What's happening, or why not. */
  message: string;
  channels: number;
  /** Samples (events, for a spiking sensor) the engine has received. */
  samples: number;
  /** Over the last poll interval. */
  samplesPerSec: number;
  batches: number;
  /** Batches that arrived too late for their place in time (the sensor's clock was re-anchored). */
  late: number;
  /** Samples dropped while paused. */
  dropped: number;
}

/** What the manager needs of the engine's client. */
export interface RealtimeClient {
  connectSensor(session: number, spec: SensorSpec): Promise<Sensor>;
  listSensors(session: number): Promise<SensorInfo[]>;
}

/** Where the run is, as far as realtime sources care. */
export interface RealtimeContext {
  client: RealtimeClient | null;
  session: number | null;
  /** The built network (targets are resolved in it). */
  network: Network | null;
  /** The input file the run uses. */
  file: InputFile | null;
  /** `idle`: not running (loaded, ready, failed); streams stop. */
  run: "running" | "paused" | "idle";
  /** A replay takes no input. */
  replay: boolean;
  /** Simulated seconds per wall second; null: as fast as possible. */
  speed: number | null;
}

/** A camera's frames are ~33 ms apart (and jitter): a batch must arrive before its first sample is due. */
const CAMERA_INGRESS_MS = 60;

const blank = (s: RealtimeSource): RealtimeStatus => ({
  name: s.name,
  kind: s.kind,
  state: "starting",
  message: "",
  channels: 0,
  samples: 0,
  samplesPerSec: 0,
  batches: 0,
  late: 0,
  dropped: 0,
});

/** The status with an engine sensor's counters: streaming once it has samples, and the rate since `previous` (the
 *  last poll's samples and time in ms). */
export function withCounters(status: RealtimeStatus, info: SensorInfo, previous: { samples: number; at: number } | null, now: number): RealtimeStatus {
  const samples = info.samples;
  const perSec = previous && now > previous.at ? ((samples - previous.samples) * 1000) / (now - previous.at) : 0;
  const streaming = samples > 0 && status.state === "starting";
  return {
    ...status,
    state: streaming ? "streaming" : status.state,
    message: streaming ? "" : status.message,
    samples,
    samplesPerSec: Math.max(0, perSec),
    batches: info.batches,
    late: info.lateBatches,
    dropped: info.dropped,
  };
}

interface Entry {
  source: RealtimeSource;
  status: RealtimeStatus;
  /** Set when the entry is stopped, so a start still waiting for a device undoes itself. */
  cancelled: boolean;
  capture?: Capture;
  sensor?: Sensor;
  polled: { samples: number; at: number } | null;
}

export class RealtimeManager {
  private entries = new Map<string, Entry>();
  private client: RealtimeClient | null = null;
  private session: number | null = null;
  /** Whether a run is going (so a new one starts with fresh entries). */
  private going = false;
  private timer: ReturnType<typeof setInterval> | undefined;

  /** `open` opens a device; `publish` gets the statuses (in the file's order) whenever one changes; `now` is the
   *  clock in ms; `pollMs` how often counters are read from the engine (0: only on `poll()`). */
  constructor(
    private readonly open: OpenCapture,
    private readonly publish: (statuses: RealtimeStatus[]) => void,
    private readonly options: { now?: () => number; pollMs?: number } = {},
  ) {}

  private now = () => (this.options.now ?? performance.now.bind(performance))();

  private emit() {
    this.publish([...this.entries.values()].map((e) => e.status));
  }

  private set(entry: Entry, change: Partial<RealtimeStatus>) {
    entry.status = { ...entry.status, ...change };
    this.emit();
  }

  private live(entry: Entry) {
    return entry.status.state === "starting" || entry.status.state === "streaming";
  }

  /**
   * Brings the streams in line with the run: sources are started when the run is running (or paused) and the speed
   * allows, refused at a speed that doesn't, and all stopped when the run isn't going or the session changes. Call it
   * whenever the context changes; it does nothing for what is already as it should be.
   */
  sync(ctx: RealtimeContext) {
    const active = ctx.client && ctx.session !== null && ctx.network && ctx.file && !ctx.replay && ctx.run !== "idle";
    if (this.session !== null && (ctx.session !== this.session || ctx.client !== this.client)) void this.stopAll();
    if (!active) {
      void this.stopAll();
      return;
    }
    if (!this.going) {
      this.going = true;
      this.entries.clear(); // a new run: the last one's final statuses go
    }
    this.client = ctx.client;
    this.session = ctx.session;
    const wanted = ctx.file!.sources.filter((s): s is RealtimeSource => s.enabled && isRealtime(s));
    for (const [name, entry] of this.entries) {
      if (!wanted.some((s) => s.name === name)) {
        void this.stop(entry);
        this.entries.delete(name);
      }
    }
    for (const s of wanted) {
      const entry = this.entries.get(s.name);
      const verdict = speedVerdict(s, ctx.speed);
      if (!verdict.ok) {
        if (entry && this.live(entry)) void this.stop(entry);
        if (!entry || entry.status.state !== "refused" || entry.status.message !== verdict.message) {
          const e = entry ?? this.add(s);
          this.set(e, { state: "refused", message: verdict.message });
        }
        continue;
      }
      if (entry && entry.status.state !== "refused") continue; // going, or failed: left as it is until the run ends
      void this.start(entry ?? this.add(s), ctx);
    }
    this.emit();
  }

  private add(source: RealtimeSource): Entry {
    const entry: Entry = { source, status: blank(source), cancelled: false, polled: null };
    this.entries.set(source.name, entry);
    return entry;
  }

  private async start(entry: Entry, ctx: RealtimeContext) {
    const s = entry.source;
    entry.cancelled = false;
    entry.status = { ...blank(s), message: s.kind === "sensor" ? "connecting" : `opening the ${s.kind}` };
    let targets: number[];
    try {
      targets = targetNeurons(s.targets, ctx.network!);
    } catch (e) {
      return this.fail(entry, e instanceof InputFileError ? e.message : String(e));
    }
    const plan = channelPlan(s, targets.length);
    if ("error" in plan) return this.fail(entry, plan.error);
    const { channels } = plan;
    entry.status.channels = channels;
    this.emit();
    const client = ctx.client!;
    try {
      if (s.kind !== "sensor") entry.capture = await this.open(s, channels);
      if (entry.cancelled) return entry.capture?.stop();
      const spec: SensorSpec = {
        name: s.name,
        targets: targets.slice(0, channels).map((n) => [n]),
        encoding: s.encoding as Encoding,
        ...(s.kind === "camera" ? { ingressDelayMs: CAMERA_INGRESS_MS } : {}),
        bufferWhilePaused: s.onPause === "buffer",
      };
      const sensor = await client.connectSensor(ctx.session!, spec);
      if (entry.cancelled) {
        entry.capture?.stop();
        return void sensor.disconnect().catch(() => {});
      }
      entry.sensor = sensor;
      this.set(entry, { message: s.kind === "sensor" ? `waiting for a program to send to the sensor named '${s.name}'` : "waiting for the first samples" });
      entry.capture?.start((t0, dt, values) => {
        try {
          sensor.sendSamples(t0, dt, channels, values);
        } catch (e) {
          this.fail(entry, `the engine is not reachable: ${e instanceof Error ? e.message : String(e)}`);
        }
      });
      this.schedule();
    } catch (e) {
      if (entry.cancelled) entry.capture?.stop();
      else this.fail(entry, e instanceof Error ? e.message : String(e));
    }
  }

  /** The entry's stream failed: it stops, with the reason. */
  private fail(entry: Entry, message: string) {
    void this.stop(entry);
    this.set(entry, { state: "error", message });
  }

  private async stop(entry: Entry) {
    entry.cancelled = true;
    const { capture, sensor } = entry;
    entry.capture = entry.sensor = undefined;
    capture?.stop();
    if (this.live(entry)) this.set(entry, { state: "stopped", message: "" });
    await sensor?.disconnect().catch(() => {}); // the engine forgets it; fine if it already has
  }

  /** Stops every stream (the run ended, or the session changed); their last counters stay shown as stopped. */
  async stopAll() {
    const entries = [...this.entries.values()];
    const { client, session } = this;
    this.going = false;
    this.session = null;
    this.client = null;
    clearInterval(this.timer);
    this.timer = undefined;
    if (!entries.some((e) => this.live(e) || e.sensor)) return;
    // One last read of the counters, so what's left on show is final.
    if (client && session !== null) await this.read(client, session, entries).catch(() => {});
    await Promise.all(entries.map((e) => this.stop(e)));
  }

  private schedule() {
    const ms = this.options.pollMs ?? 0;
    if (ms > 0 && !this.timer) this.timer = setInterval(() => void this.poll(), ms);
  }

  /** Reads the sensors' counters from the engine: samples, rate, late and dropped batches. */
  async poll() {
    if (this.client && this.session !== null) await this.read(this.client, this.session, [...this.entries.values()]).catch(() => {});
  }

  private async read(client: RealtimeClient, session: number, entries: Entry[]) {
    const list = await client.listSensors(session);
    const now = this.now();
    for (const entry of entries) {
      if (!this.live(entry) || !entry.sensor) continue;
      // The newest sensor of the source's name (the highest key).
      const info = list.filter((x) => x.name === entry.source.name).sort((a, b) => b.id - a.id)[0];
      if (!info) continue;
      entry.status = withCounters(entry.status, info, entry.polled, now);
      entry.polled = { samples: info.samples, at: now };
    }
    this.emit();
  }
}
