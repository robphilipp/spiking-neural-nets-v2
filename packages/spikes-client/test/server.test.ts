// End to end: the client against a real spikes-server (built by `cargo build -p spikes-server`).

import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { type Frame, type Notice, spikeCount, SpikesError, WebSocketClient } from "../src";

const root = join(__dirname, "..", "..", "..");
const binary = join(root, "target", "debug", "spikes-server");
const ring = readFileSync(join(root, "test-data", "v1", "ring.boo"), "utf8");

interface Server {
  url: string;
  process: ChildProcess;
  recordings: string;
}

async function startServer(extra: string[] = []): Promise<Server> {
  if (!existsSync(binary)) throw new Error(`build the server first: cargo build -p spikes-server (${binary})`);
  const recordings = mkdtempSync(join(tmpdir(), "spikes-recordings-"));
  const child = spawn(binary, ["--listen", "127.0.0.1:0", "--recordings", recordings, ...extra]);
  const url = await new Promise<string>((resolve, reject) => {
    let out = "";
    child.stdout!.on("data", (chunk: Buffer) => {
      out += chunk.toString();
      const m = out.match(/listening on (http:\/\/[\d.:]+)/);
      if (m) resolve(m[1]!);
    });
    child.on("exit", (code) => reject(new Error(`server exited (${code}): ${out}`)));
    setTimeout(() => reject(new Error(`server didn't start: ${out}`)), 10_000);
  });
  return { url, process: child, recordings };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Waits until `check` returns something, or fails after `ms`. */
async function until<T>(check: () => T | undefined, ms = 5000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const value = check();
    if (value !== undefined) return value;
    if (Date.now() > deadline) throw new Error("timed out");
    await sleep(10);
  }
}

let server: Server;
beforeAll(async () => {
  server = await startServer();
});
afterAll(() => {
  server?.process.kill();
});

describe("a session over WebSocket", () => {
  it("builds, runs, records, steps and stops", async () => {
    const client = await WebSocketClient.connect(server.url);
    const notices: Notice[] = [];
    client.onNotice((_, n) => notices.push(n));

    const { id } = await client.createSession("ring", ring);
    await client.build(id, { shards: 2 });
    const network = await client.getNetwork(id);
    expect(network.neurons).toHaveLength(25);

    const frames: Frame[] = [];
    await client.attach(id, { frameMs: 16, budgetBytesPerS: 4 << 20, spikes: { type: "all" }, membrane: [], weights: { type: "nothing" } }, (f) => frames.push(f));
    const group1 = network.neurons.filter((n) => n.name.startsWith("hidden-10")).map((n) => n.index);
    await client.stimulate(id, group1, 10);
    await client.start(id, { durationMs: 200, record: true });
    const ended = await until(() => notices.find((n) => n.type === "runEnded"));
    expect(ended).toMatchObject({ type: "runEnded", simMs: 200 });
    await until(() => (frames.reduce((n, f) => n + spikeCount(f), 0) >= 25 ? true : undefined));
    expect(frames.reduce((n, f) => n + spikeCount(f), 0)).toBe(25);

    // The recording is listed and downloadable.
    const recording = (ended as Extract<Notice, { type: "runEnded" }>).recording!;
    const recordings = await client.listRecordings();
    expect(recordings.find((r) => r.id === recording)?.files.map((f) => f.name)).toContain("spikes.arrows");
    const download = await fetch(`${server.url}/api/recordings/${recording}/spikes.arrows`);
    expect(download.ok).toBe(true);
    expect((await download.arrayBuffer()).byteLength).toBeGreaterThan(100);

    expect((await client.queryNeuron(id, group1[0]!)).spikes).toBe(1);
    await expect(client.pause(id)).rejects.toMatchObject({ error: { code: "invalidState" } });

    // Paced: pause, step, resume, change speed, stop.
    await client.start(id, { speed: 1 });
    await sleep(50);
    await client.pause(id);
    await sleep(20);
    const before = (await client.listSessions()).find((s) => s.id === id)!.simMs;
    await client.step(id, 5);
    await sleep(50);
    const after = (await client.listSessions()).find((s) => s.id === id)!;
    expect(after.state).toBe("paused");
    expect(after.simMs).toBeCloseTo(before + 5, 6);
    await client.resume(id);
    await client.setSpeed(id, 4);
    await client.stop(id);
    expect((await client.listSessions()).find((s) => s.id === id)!.state).toBe("ready");
    client.close();
  });

  it("reports parse errors with positions", async () => {
    const client = await WebSocketClient.connect(server.url);
    const error = await client.createSession("bad", "(NRN=[(nid=a, mst=1 parsec)])").catch((e) => e);
    expect(error).toBeInstanceOf(SpikesError);
    expect(error.error.code).toBe("parse");
    expect(error.error.spans[0]).toMatchObject({ line: 1 });
    client.close();
  });

  it("answers control requests within a few milliseconds", async () => {
    const client = await WebSocketClient.connect(server.url);
    const times: number[] = [];
    for (let i = 0; i < 50; i++) {
      const t = performance.now();
      await client.listSessions();
      times.push(performance.now() - t);
    }
    times.sort((a, b) => a - b);
    expect(times[25]).toBeLessThan(5);
    client.close();
  });
});

describe("several clients", () => {
  it("watch one session; one leaving doesn't affect the other", async () => {
    const a = await WebSocketClient.connect(server.url);
    const b = await WebSocketClient.connect(server.url);
    const { id } = await a.createSession("shared", ring);
    await a.build(id);
    const view = { frameMs: 16, budgetBytesPerS: 4 << 20, spikes: { type: "all" as const }, membrane: [], weights: { type: "nothing" as const } };
    const seenA: Frame[] = [];
    const seenB: Frame[] = [];
    await a.attach(id, view, (f) => seenA.push(f));
    await b.attach(id, view, (f) => seenB.push(f));
    await a.start(id, { speed: 1 });
    await until(() => (seenA.length > 3 && seenB.length > 3 ? true : undefined));
    b.close();
    const countA = seenA.length;
    await sleep(200);
    expect(seenA.length).toBeGreaterThan(countA + 5);
    const sessions = await a.listSessions();
    expect(sessions.find((s) => s.id === id)!.state).toBe("running");
    await a.stop(id);
    a.close();
  });
});

describe("sensors", () => {
  it("drive neurons with binary sample batches", async () => {
    const client = await WebSocketClient.connect(server.url);
    const { id } = await client.createSession("sensed", ring);
    await client.build(id);
    const fired = new Set<number>();
    await client.attach(id, { frameMs: 16, budgetBytesPerS: 4 << 20, spikes: { type: "all" }, membrane: [], weights: { type: "nothing" } }, (f) => {
      for (const n of f.spikes?.neuron ?? []) fired.add(n);
    });
    const sensor = await client.connectSensor(id, { name: "probe", targets: [[0], [1]], encoding: { type: "current", gainMv: 20 }, bufferWhilePaused: false });
    await client.start(id, { speed: 1 });
    sensor.sendSamples(0n, 1_000_000, 2, [1, 1]);
    await until(() => (fired.has(0) && fired.has(1) ? true : undefined));
    const errors: string[] = [];
    client.onError((e) => errors.push(e.code));
    // A batch for a sensor that doesn't exist is reported without a request.
    (client as unknown as { sendBinary(b: ArrayBuffer): void }).sendBinary(new ArrayBuffer(8));
    await until(() => (errors.length ? true : undefined));
    expect(errors[0]).toBe("invalid");
    await sensor.disconnect();
    await client.stop(id);
    client.close();
  });
});

describe("access tokens", () => {
  it("are required when configured", async () => {
    const dir = mkdtempSync(join(tmpdir(), "spikes-token-"));
    writeFileSync(join(dir, "token"), "s3cret\n");
    const guarded = await startServer(["--token-file", join(dir, "token")]);
    try {
      await expect(WebSocketClient.connect(guarded.url, { reconnect: false })).rejects.toThrow();
      const client = await WebSocketClient.connect(guarded.url, { token: "s3cret" });
      expect(await client.listSessions()).toEqual([]);
      client.close();
    } finally {
      guarded.process.kill();
    }
  });
});
