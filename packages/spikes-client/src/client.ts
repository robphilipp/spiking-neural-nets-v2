// The protocol client. `ProtocolClient` holds everything but the transport; `WebSocketClient` adds a WebSocket
// (and a Tauri client will add Tauri IPC), so the UI uses one API either way.

import { decodeFrame, type Frame } from "./frame";
import { encodeEvents, encodeSamples } from "./input";
import type { ApiError } from "./generated/ApiError";
import type { BuildSettings } from "./generated/BuildSettings";
import type { ClientMessage } from "./generated/ClientMessage";
import type { Cluster } from "./generated/Cluster";
import type { Input } from "./generated/Input";
import type { Network } from "./generated/Network";
import type { NeuronState } from "./generated/NeuronState";
import type { Notice } from "./generated/Notice";
import type { Recording } from "./generated/Recording";
import type { Request } from "./generated/Request";
import type { Response } from "./generated/Response";
import type { SavedRun } from "./generated/SavedRun";
import type { SensorInfo } from "./generated/SensorInfo";
import type { SensorSpec } from "./generated/SensorSpec";
import type { ServerMessage } from "./generated/ServerMessage";
import type { SessionSummary } from "./generated/SessionSummary";
import type { View } from "./generated/View";

/** A request the engine refused. */
export class SpikesError extends Error {
  constructor(readonly error: ApiError) {
    super(error.message);
    this.name = "SpikesError";
  }
}

export type Status = "connecting" | "connected" | "disconnected";

/** A live view: frames go to its callback until it's detached. */
export interface Viewer {
  readonly id: number;
  update(view: View): Promise<void>;
  detach(): Promise<void>;
}

/** A connected sensor. Batches are sent without waiting for an answer; errors arrive as `onError`. */
export interface Sensor {
  readonly id: number;
  /** Samples on all channels at `dtNs` spacing, sample-major; `t0Ns` is the sensor's own clock. */
  sendSamples(t0Ns: bigint, dtNs: number, channels: number, values: ArrayLike<number>): void;
  /** Events: a channel and an offset (ns from `t0Ns`) each. */
  sendEvents(t0Ns: bigint, channels: ArrayLike<number>, offsetsNs: ArrayLike<number>): void;
  disconnect(): Promise<void>;
}

export interface StartOptions {
  /** Simulated seconds per wall second; omit or `null` for as fast as possible. */
  speed?: number | null;
  durationMs?: number | null;
  record?: boolean;
  /** On nodes, at a set speed: keep every node in lockstep (exact; the server's default) or let each follow its
   *  copy of the clock (best effort). */
  lockstep?: boolean;
}

type Pending = { resolve: (r: Response) => void; reject: (e: Error) => void };

/** Everything but the transport. */
export abstract class ProtocolClient {
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly viewers = new Map<string, (frame: Frame) => void>();
  private readonly noticeHandlers = new Set<(session: number, notice: Notice) => void>();
  private readonly errorHandlers = new Set<(error: ApiError) => void>();
  private sensorSeq = 0;

  protected abstract sendText(text: string): void;
  protected abstract sendBinary(data: ArrayBuffer): void;

  /** Call with each text message from the engine. */
  protected receiveText(text: string): void {
    const message = JSON.parse(text) as ServerMessage;
    if (message.type === "notice") {
      for (const h of this.noticeHandlers) h(message.session, message.notice);
      return;
    }
    const waiting = this.pending.get(message.id);
    if (waiting) {
      this.pending.delete(message.id);
      if ("ok" in message.result) waiting.resolve(message.result.ok);
      else waiting.reject(new SpikesError(message.result.error));
    } else if ("error" in message.result) {
      // Errors without a request (id 0): malformed messages, bad sensor batches.
      for (const h of this.errorHandlers) h(message.result.error);
    }
  }

  /** Call with each binary message (a frame) from the engine. */
  protected receiveBinary(data: ArrayBuffer | Uint8Array): void {
    const frame = decodeFrame(data);
    this.viewers.get(`${frame.header.session}:${frame.header.viewer}`)?.(frame);
  }

  /** Call when the transport goes away: pending requests fail; viewers and sensors are gone on the engine side. */
  protected connectionLost(): void {
    for (const p of this.pending.values()) p.reject(new Error("disconnected"));
    this.pending.clear();
    this.viewers.clear();
  }

  /** Sends a request and waits for its response. */
  request(request: Request): Promise<Response> {
    const id = this.nextId++;
    const message: ClientMessage = { id, request };
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try {
        this.sendText(JSON.stringify(message));
      } catch (e) {
        this.pending.delete(id);
        reject(e instanceof Error ? e : new Error(String(e)));
      }
    });
  }

  private async expect<T extends Response["type"]>(request: Request, type: T): Promise<Extract<Response, { type: T }>> {
    const response = await this.request(request);
    if (response.type !== type) throw new Error(`expected a ${type} response, got ${response.type}`);
    return response as Extract<Response, { type: T }>;
  }

  /** Session notices (state changes, run ends, failures). Returns an unsubscribe function. */
  onNotice(handler: (session: number, notice: Notice) => void): () => void {
    this.noticeHandlers.add(handler);
    return () => this.noticeHandlers.delete(handler);
  }

  /** Errors not tied to a request (e.g. a rejected sensor batch). Returns an unsubscribe function. */
  onError(handler: (error: ApiError) => void): () => void {
    this.errorHandlers.add(handler);
    return () => this.errorHandlers.delete(handler);
  }

  async listSessions(): Promise<SessionSummary[]> {
    return (await this.expect({ type: "listSessions" }, "sessions")).sessions;
  }

  async createSession(name: string, description: string): Promise<SessionSummary> {
    return (await this.expect({ type: "createSession", name, description }, "session")).session;
  }

  async updateDescription(session: number, description: string): Promise<void> {
    await this.request({ type: "updateDescription", session, description });
  }

  async closeSession(session: number): Promise<void> {
    await this.request({ type: "closeSession", session });
  }

  async build(session: number, settings: BuildSettings = {}): Promise<void> {
    await this.request({ type: "build", session, settings });
  }

  async getDescription(session: number): Promise<string> {
    return (await this.expect({ type: "getDescription", session }, "description")).text;
  }

  /** How a session built on node agents is doing. */
  async getCluster(session: number): Promise<Cluster> {
    return (await this.expect({ type: "getCluster", session }, "cluster")).cluster;
  }

  /** The sensors connected to a session (from any client), with their counters. */
  async listSensors(session: number): Promise<SensorInfo[]> {
    return (await this.expect({ type: "listSensors", session }, "sensors")).sensors;
  }

  async getNetwork(session: number): Promise<Network> {
    return (await this.expect({ type: "getNetwork", session }, "network")).network;
  }

  async start(session: number, options: StartOptions = {}): Promise<void> {
    await this.request({
      type: "start",
      session,
      speed: options.speed ?? null,
      durationMs: options.durationMs ?? null,
      record: options.record ?? false,
      ...(options.lockstep === undefined ? {} : { lockstep: options.lockstep }),
    });
  }

  async pause(session: number): Promise<void> {
    await this.request({ type: "pause", session });
  }

  async resume(session: number): Promise<void> {
    await this.request({ type: "resume", session });
  }

  async step(session: number, ms: number): Promise<void> {
    await this.request({ type: "step", session, ms });
  }

  /** `null` for as fast as possible. */
  async setSpeed(session: number, speed: number | null): Promise<void> {
    await this.request({ type: "setSpeed", session, speed });
  }

  async stop(session: number): Promise<void> {
    await this.request({ type: "stop", session });
  }

  async stimulate(session: number, neurons: number[], mv: number): Promise<void> {
    await this.request({ type: "stimulate", session, neurons, mv });
  }

  async releaseModulator(session: number, modulator: number, amount: number): Promise<void> {
    await this.request({ type: "releaseModulator", session, modulator, amount });
  }

  async addInput(session: number, input: Input): Promise<void> {
    await this.request({ type: "addInput", session, input });
  }

  /** Adds a connection now (also while running); `inhibitory` and `learning` default to the sender's sign and none. */
  async connect(session: number, pre: number, post: number, weight: number, options: { inhibitory?: boolean; learning?: string } = {}): Promise<void> {
    await this.request({ type: "connect", session, pre, post, weight, inhibitory: options.inhibitory ?? null, learning: options.learning ?? null });
  }

  /** Removes a connection (also while running). */
  async prune(session: number, pre: number, post: number): Promise<void> {
    await this.request({ type: "prune", session, pre, post });
  }

  async queryNeuron(session: number, neuron: number): Promise<NeuronState> {
    return (await this.expect({ type: "queryNeuron", session, neuron }, "neuron")).neuron;
  }

  async observe(session: number, neurons: number[], on = true): Promise<void> {
    await this.request({ type: "observe", session, neurons, on });
  }

  async listRecordings(): Promise<Recording[]> {
    return (await this.expect({ type: "listRecordings" }, "recordings")).recordings;
  }

  /** Opens a recorded run as a replay session (build and start it like a run). */
  async openRecording(id: string): Promise<SessionSummary> {
    return (await this.expect({ type: "openRecording", id }, "session")).session;
  }

  /** Converts a recorded run to Parquet, beside its Arrow files. */
  async exportParquet(id: string): Promise<void> {
    await this.request({ type: "exportParquet", id });
  }

  /** Saves a session's run so far (when ready: built, or stopped), to continue later with `openSave`. */
  async saveRun(session: number, name: string): Promise<SavedRun> {
    return (await this.expect({ type: "saveRun", session, name }, "saved")).save;
  }

  /** Saved runs, oldest first. */
  async listSaves(): Promise<SavedRun[]> {
    return (await this.expect({ type: "listSaves" }, "saves")).saves;
  }

  /** Opens a saved run as a new session, ready at the saved time (start it to continue). */
  async openSave(id: string, shards?: number): Promise<SessionSummary> {
    return (await this.expect({ type: "openSave", id, shards: shards ?? null }, "session")).session;
  }

  /** A replay: jumps to simulated time `ms` (not while running). */
  async seek(session: number, ms: number): Promise<void> {
    await this.request({ type: "seek", session, ms });
  }

  /** Starts a live view; `onFrame` gets each frame until the view is detached. */
  async attach(session: number, view: View, onFrame: (frame: Frame) => void): Promise<Viewer> {
    const { viewer: id } = await this.expect({ type: "attach", session, view }, "viewer");
    const key = `${session}:${id % 65536}`;
    this.viewers.set(key, onFrame);
    return {
      id,
      update: async (next: View) => {
        await this.request({ type: "updateView", session, viewer: id, view: next });
      },
      detach: async () => {
        this.viewers.delete(key);
        await this.request({ type: "detach", session, viewer: id });
      },
    };
  }

  /** Connects an external sensor. */
  async connectSensor(session: number, spec: SensorSpec): Promise<Sensor> {
    const { sensor: id } = await this.expect({ type: "connectSensor", session, sensor: spec }, "sensor");
    return {
      id,
      sendSamples: (t0Ns, dtNs, channels, values) =>
        this.sendBinary(encodeSamples(id, this.sensorSeq++, t0Ns, dtNs, channels, values)),
      sendEvents: (t0Ns, channels, offsetsNs) =>
        this.sendBinary(encodeEvents(id, this.sensorSeq++, t0Ns, channels, offsetsNs)),
      disconnect: async () => {
        await this.request({ type: "disconnectSensor", session, sensor: id });
      },
    };
  }
}

export interface WebSocketOptions {
  /** Access token, if the server needs one. */
  token?: string;
  /** Reconnect after the connection drops (default true). Viewers and sensors must be set up again. */
  reconnect?: boolean;
}

/** A client over a WebSocket (`ws://host:port`, the server's address). */
export class WebSocketClient extends ProtocolClient {
  private socket?: WebSocket;
  private statusValue: Status = "connecting";
  private readonly statusHandlers = new Set<(s: Status) => void>();
  private closed = false;
  private retryMs = 250;

  private constructor(private readonly url: string, private readonly options: WebSocketOptions) {
    super();
  }

  /** Connects; resolves once the connection is open. */
  static async connect(server: string, options: WebSocketOptions = {}): Promise<WebSocketClient> {
    const base = server.replace(/^http/, "ws").replace(/\/$/, "");
    const params = new URLSearchParams({ v: "1" });
    if (options.token) params.set("token", options.token);
    const client = new WebSocketClient(`${base}/api/ws?${params}`, options);
    await client.open();
    return client;
  }

  get status(): Status {
    return this.statusValue;
  }

  /** Status changes. Returns an unsubscribe function. */
  onStatus(handler: (s: Status) => void): () => void {
    this.statusHandlers.add(handler);
    return () => this.statusHandlers.delete(handler);
  }

  private setStatus(s: Status) {
    this.statusValue = s;
    for (const h of this.statusHandlers) h(s);
  }

  private open(): Promise<void> {
    return new Promise((resolve, reject) => {
      const socket = new WebSocket(this.url);
      socket.binaryType = "arraybuffer";
      let opened = false;
      socket.onopen = () => {
        opened = true;
        this.socket = socket;
        this.retryMs = 250;
        this.setStatus("connected");
        resolve();
      };
      socket.onmessage = (event: MessageEvent) => {
        if (typeof event.data === "string") this.receiveText(event.data);
        else this.receiveBinary(event.data as ArrayBuffer);
      };
      socket.onclose = () => {
        this.connectionLost();
        if (!opened) {
          reject(new Error(`couldn't connect to ${this.url.replace(/token=[^&]*/, "token=…")}`));
          return;
        }
        if (this.closed || this.options.reconnect === false) {
          this.setStatus("disconnected");
          return;
        }
        this.setStatus("connecting");
        const retry = () =>
          this.open().catch(() => {
            this.retryMs = Math.min(this.retryMs * 2, 5000);
            setTimeout(retry, this.retryMs);
          });
        setTimeout(retry, this.retryMs);
      };
    });
  }

  protected sendText(text: string): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) throw new Error("not connected");
    this.socket.send(text);
  }

  protected sendBinary(data: ArrayBuffer): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) throw new Error("not connected");
    this.socket.send(data);
  }

  /** Closes the connection (the engine detaches this client's viewers and sensors). */
  close(): void {
    this.closed = true;
    this.socket?.close();
  }
}
