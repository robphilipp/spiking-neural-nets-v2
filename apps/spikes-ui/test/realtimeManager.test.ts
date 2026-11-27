import type { Network, Sensor, SensorInfo, SensorSpec } from "@spikes/client";
import { describe, expect, it } from "vitest";
import type { Capture, Emit } from "../src/capture";
import { type InputFile, parseInputFile } from "../src/inputFile";
import { type RealtimeClient, type RealtimeContext, RealtimeManager, type RealtimeStatus, withCounters } from "../src/realtimeManager";

/** A network of `n` neurons named a0, a1, …, b0, … (first half "a", second half "b"). */
function network(n: number): Network {
  return {
    neurons: Array.from({ length: n }, (_, i) => ({ index: i, name: `${i < n / 2 ? "a" : "b"}${i}` })),
    sets: [],
  } as unknown as Network;
}

const FILE = (extra = "") => `
[[source]]
name = "voice"
kind = "microphone"
neurons = { prefix = "a" }
encoding = { kind = "rate", max_hz = 200, mv = 12 }

[[source]]
name = "eye"
kind = "camera"
neurons = { prefix = "b" }
grid = "2x2"
encoding = { kind = "current", gain_mv = 5 }
${extra}

[[source]]
name = "arm"
kind = "sensor"
neurons = { prefix = "a" }
encoding = { kind = "spikes", mv = 10 }
`;

class Fake implements RealtimeClient {
  sensors: Array<{ spec: SensorSpec; id: number; disconnected: boolean; sent: Array<[bigint, number, number, number]> }> = [];
  samples = new Map<string, number>();
  extra = { batches: 0, lateBatches: 0, dropped: 0 };
  async connectSensor(_session: number, spec: SensorSpec): Promise<Sensor> {
    const rec = { spec, id: this.sensors.length + 1, disconnected: false, sent: [] as Array<[bigint, number, number, number]> };
    this.sensors.push(rec);
    return {
      id: rec.id,
      sendSamples: (t0, dt, channels, values) => rec.sent.push([t0, dt, channels, values.length]),
      sendEvents: () => {},
      disconnect: async () => void (rec.disconnected = true),
    };
  }
  async listSensors(): Promise<SensorInfo[]> {
    return this.sensors
      .filter((s) => !s.disconnected)
      .map((s) => ({ id: s.id, name: s.spec.name, channels: s.spec.targets, encoding: "rate", samples: this.samples.get(s.spec.name) ?? 0, inputs: 0, ...this.extra }));
  }
}

class Devices {
  opened: Array<{ kind: string; channels: number; stopped: boolean; emit?: Emit }> = [];
  fail: string | null = null;
  open = async (source: { kind: string }, channels: number): Promise<Capture> => {
    if (this.fail) throw new Error(this.fail);
    const rec: { kind: string; channels: number; stopped: boolean; emit?: Emit } = { kind: source.kind, channels, stopped: false };
    this.opened.push(rec);
    return { channels, start: (emit) => void (rec.emit = emit), stop: () => void (rec.stopped = true) };
  };
}

function setup(text = FILE(), neurons = 20) {
  const client = new Fake();
  const devices = new Devices();
  let statuses: RealtimeStatus[] = [];
  let clock = 0;
  const manager = new RealtimeManager(devices.open as never, (s) => (statuses = s), { now: () => clock });
  const file: InputFile = parseInputFile(text);
  const ctx: RealtimeContext = { client, session: 1, network: network(neurons), file, run: "running", replay: false, speed: 1 };
  const by = (name: string) => statuses.find((s) => s.name === name)!;
  return { client, devices, manager, ctx, by, all: () => statuses, tick: (ms: number) => (clock += ms) };
}

/** The names of the sensors connected, sorted (a source with a device to open connects later than one without). */
const names = (c: Fake) => c.sensors.map((s) => s.spec.name).sort();

const settle = () => new Promise((r) => setTimeout(r, 0));

describe("the realtime manager", () => {
  it("starts every enabled realtime source when the run is running, as sensors named after them", async () => {
    const { client, devices, manager, ctx, by, all } = setup();
    manager.sync(ctx);
    await settle();
    expect(all().map((s) => s.name)).toEqual(["voice", "eye", "arm"]);
    // 20 neurons: a0…a9 and b10…b19. The microphone: a band per "a" neuron; the camera: 2×2 = 4 channels from the
    // first 4 "b" neurons; the sensor: a channel per "a" neuron.
    const sensor = (name: string) => client.sensors.find((s) => s.spec.name === name)!.spec;
    expect(client.sensors.map((s) => [s.spec.name, s.spec.targets.length, s.spec.bufferWhilePaused]).sort()).toEqual([
      ["arm", 10, false],
      ["eye", 4, false],
      ["voice", 10, false],
    ]);
    expect(sensor("eye").targets).toEqual([[10], [11], [12], [13]]);
    expect(sensor("eye").ingressDelayMs).toBe(60);
    expect(sensor("voice").ingressDelayMs).toBeUndefined();
    expect(devices.opened.map((d) => [d.kind, d.channels])).toEqual([
      ["microphone", 10],
      ["camera", 4],
    ]); // a sensor source opens no device
    expect(by("voice").state).toBe("starting"); // nothing received yet
    expect(by("arm").message).toMatch(/waiting for a program to send to the sensor named 'arm'/);
  });

  it("sends the capture's batches to its sensor", async () => {
    const { client, devices, manager, ctx } = setup();
    manager.sync(ctx);
    await settle();
    devices.opened[0]!.emit!(5n, 10_000_000, new Float32Array(30)); // 3 samples of 10 channels
    expect(client.sensors.find((s) => s.spec.name === "voice")!.sent).toEqual([[5n, 10_000_000, 10, 30]]);
  });

  it("shows a source streaming once the engine has its samples, with their rate", async () => {
    const { client, manager, ctx, by, tick } = setup();
    manager.sync(ctx);
    await settle();
    client.samples.set("voice", 100);
    tick(1000);
    await manager.poll();
    expect(by("voice")).toMatchObject({ state: "streaming", samplesPerSec: 0 }); // first reading: samples arrived, no rate yet
    client.samples.set("voice", 300);
    client.extra = { batches: 30, lateBatches: 2, dropped: 7 };
    tick(1000);
    await manager.poll();
    expect(by("voice")).toMatchObject({ state: "streaming", samples: 300, samplesPerSec: 200, batches: 30, late: 2, dropped: 7 });
    expect(by("eye").state).toBe("starting"); // no samples yet
  });

  it("holds the streams across pause and resume, and stops them when the run ends", async () => {
    const { client, devices, manager, ctx, by } = setup();
    manager.sync(ctx);
    await settle();
    client.samples.set("voice", 10);
    await manager.poll();
    manager.sync({ ...ctx, run: "paused" });
    manager.sync({ ...ctx, run: "running" });
    await settle();
    expect(client.sensors).toHaveLength(3); // nothing restarted
    expect(devices.opened.every((d) => !d.stopped)).toBe(true);
    manager.sync({ ...ctx, run: "idle" });
    await settle();
    expect(devices.opened.every((d) => d.stopped)).toBe(true);
    expect(client.sensors.every((s) => s.disconnected)).toBe(true);
    expect(by("voice")).toMatchObject({ state: "stopped", samples: 10 }); // its last counters stay
  });

  it("a buffering source tells the engine to buffer while paused", async () => {
    const { client, manager, ctx } = setup(FILE().replace('name = "voice"', 'name = "voice"\non_pause = "buffer"'));
    manager.sync(ctx);
    await settle();
    expect(client.sensors.find((s) => s.spec.name === "voice")!.spec.bufferWhilePaused).toBe(true);
  });

  it("starts nothing for a run as fast as possible, and says why", async () => {
    const { client, manager, ctx, all } = setup();
    manager.sync({ ...ctx, speed: null });
    await settle();
    expect(client.sensors).toHaveLength(0);
    expect(all().map((s) => s.state)).toEqual(["refused", "refused", "refused"]);
    expect(all()[0]!.message).toMatch(/as fast as possible/);
  });

  it("starts them when the speed becomes a set one mid-run, and stops them when it becomes max", async () => {
    const { client, devices, manager, ctx, all } = setup();
    manager.sync({ ...ctx, speed: null });
    manager.sync({ ...ctx, speed: 2 });
    await settle();
    expect(client.sensors).toHaveLength(3);
    expect(all().map((s) => s.state)).toEqual(["starting", "starting", "starting"]);
    manager.sync({ ...ctx, speed: null });
    await settle();
    expect(all().map((s) => s.state)).toEqual(["refused", "refused", "refused"]);
    expect(devices.opened.every((d) => d.stopped)).toBe(true);
    expect(client.sensors.every((s) => s.disconnected)).toBe(true);
  });

  it("a source with speed = refuse runs only at ×1", async () => {
    const text = FILE().replace('name = "voice"', 'name = "voice"\nspeed = "refuse"');
    const { client, manager, ctx, by } = setup(text);
    manager.sync({ ...ctx, speed: 0.5 });
    await settle();
    expect(by("voice")).toMatchObject({ state: "refused", message: "the run is at speed ×0.5 and this source runs only at ×1" });
    expect(by("eye").state).toBe("starting"); // the others scale their time by the speed
    expect(names(client)).toEqual(["arm", "eye"]);
    manager.sync({ ...ctx, speed: 1 });
    await settle();
    expect(by("voice").state).toBe("starting");
    manager.sync({ ...ctx, speed: 4 });
    await settle();
    expect(by("voice").state).toBe("refused");
  });

  it("refuses a camera with too few target neurons, with the reason", async () => {
    const { client, manager, ctx, by } = setup(FILE().replace('grid = "2x2"', 'grid = "16x16"'));
    manager.sync(ctx);
    await settle();
    expect(by("eye")).toMatchObject({ state: "error", message: "camera 16x16 needs 256 target neurons, has 10" });
    expect(names(client)).toEqual(["arm", "voice"]); // the others went ahead
  });

  it("reports a missing device or refused access on that source alone", async () => {
    const { client, devices, manager, ctx, by } = setup();
    devices.fail = "access to the microphone was refused";
    manager.sync(ctx);
    await settle();
    expect(by("voice")).toMatchObject({ state: "error", message: "access to the microphone was refused" });
    expect(by("arm").state).toBe("starting");
    expect(names(client)).toEqual(["arm"]);
  });

  it("reports unknown targets", async () => {
    const { manager, ctx, by } = setup(FILE().replace('neurons = { prefix = "b" }', 'neurons = { set = "missing" }'));
    manager.sync(ctx);
    await settle();
    expect(by("eye")).toMatchObject({ state: "error", message: "the network has no set 'missing'" });
  });

  it("skips disabled sources, and does nothing for a replay or no network", async () => {
    const { client, manager, ctx, all } = setup(FILE().replace('name = "arm"', 'name = "arm"\nenabled = false'));
    manager.sync(ctx);
    await settle();
    expect(all().map((s) => s.name)).toEqual(["voice", "eye"]);
    const other = setup();
    other.manager.sync({ ...other.ctx, replay: true });
    other.manager.sync({ ...other.ctx, network: null });
    other.manager.sync({ ...other.ctx, client: null });
    await settle();
    expect(other.all()).toEqual([]);
    expect(client.sensors).toHaveLength(2);
  });

  it("stops everything when the session changes, and starts afresh for the next run", async () => {
    const { client, manager, ctx, all } = setup();
    manager.sync(ctx);
    await settle();
    manager.sync({ ...ctx, session: 2 });
    await settle();
    expect(client.sensors).toHaveLength(6); // the old three went, three new
    expect(client.sensors.slice(0, 3).every((s) => s.disconnected)).toBe(true);
    expect(client.sensors.slice(3).every((s) => !s.disconnected)).toBe(true);
    manager.sync({ ...ctx, session: 2, run: "idle" });
    await settle();
    manager.sync({ ...ctx, session: 2 });
    await settle();
    expect(all().map((s) => s.state)).toEqual(["starting", "starting", "starting"]);
    expect(client.sensors).toHaveLength(9);
  });

  it("undoes a start that is still waiting for a device when the run ends", async () => {
    const { client, manager, ctx } = setup();
    const slow = new Devices();
    const open = slow.open;
    let release: () => void = () => {};
    slow.open = async (...args) => {
      await new Promise<void>((r) => (release = r));
      return open(...args);
    };
    const m = new RealtimeManager(slow.open as never, () => {});
    m.sync(ctx);
    m.sync({ ...ctx, run: "idle" });
    release();
    await settle();
    expect(slow.opened.every((d) => d.stopped)).toBe(true);
    expect(client.sensors.filter((s) => !s.disconnected && s.spec.name === "voice")).toHaveLength(0);
    void manager;
  });

  it("fails a source whose sends fail", async () => {
    const { client, devices, manager, ctx, by } = setup();
    const connect = client.connectSensor.bind(client);
    client.connectSensor = async (s, spec) => {
      const sensor = await connect(s, spec);
      return {
        ...sensor,
        sendSamples: () => {
          throw new Error("not connected");
        },
      };
    };
    manager.sync(ctx);
    await settle();
    devices.opened[0]!.emit!(0n, 1, new Float32Array(10));
    expect(by("voice").state).toBe("error");
    expect(by("voice").message).toMatch(/not connected/);
    expect(devices.opened[0]!.stopped).toBe(true);
  });
});

describe("a status with the engine's counters", () => {
  const base: RealtimeStatus = { name: "x", kind: "sensor", state: "starting", message: "waiting", channels: 2, samples: 0, samplesPerSec: 0, batches: 0, late: 0, dropped: 0 };
  const info = (samples: number): SensorInfo => ({ id: 1, name: "x", channels: [], encoding: "rate", samples, inputs: 0, batches: 3, lateBatches: 1, dropped: 2 });

  it("streams once it has samples, and never goes back", () => {
    expect(withCounters(base, info(0), null, 0).state).toBe("starting");
    const s = withCounters(base, info(50), null, 0);
    expect(s).toMatchObject({ state: "streaming", message: "", samples: 50, batches: 3, late: 1, dropped: 2 });
    expect(withCounters(s, info(50), { samples: 50, at: 0 }, 1000)).toMatchObject({ state: "streaming", samplesPerSec: 0 });
  });

  it("computes samples per second from the last reading", () => {
    expect(withCounters(base, info(150), { samples: 50, at: 1000 }, 1500).samplesPerSec).toBe(200);
  });
});
