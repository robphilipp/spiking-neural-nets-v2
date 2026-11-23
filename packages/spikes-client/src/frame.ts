// Live-data frames (see crates/spikes-api/src/frame.rs for the layout). Columns are returned as typed-array views
// over the received bytes, without copying.

export const MAGIC = "SPKF";
const HEADER_BYTES = 64;
const SECTION_HEADER_BYTES = 16;

export enum Lod {
  /** Every spike, with its time. */
  Exact = 0,
  /** Spikes per neuron in the frame. */
  Counts = 1,
  /** Spikes per spatial cell in the frame. */
  Binned = 2,
}

export interface FrameHeader {
  version: number;
  session: number;
  seq: number;
  /** Which of the client's views (its id modulo 2^16). */
  viewer: number;
  t0Ns: bigint;
  t1Ns: bigint;
  /** Start of the simulated interval, ms. */
  t0Ms: number;
  /** End of the simulated interval, ms. */
  t1Ms: number;
  wallUnixMs: number;
  /** Simulated seconds per wall second; Infinity as fast as possible; 0 when paused or stopped. */
  speed: number;
  /** Simulation ms behind the clock. */
  lagMs: number;
  lod: Lod;
  droppedEvents: number;
  droppedFrames: number;
  /** Offsets are in this unit, ns (1 unless the frame spans more than ~4.3 s). */
  timeUnitNs: number;
}

export interface Frame {
  header: FrameHeader;
  spikes?: { neuron: Uint32Array; offset: Uint32Array };
  spikeCounts?: { neuron: Uint32Array; count: Uint16Array };
  spikeBins?: { cell: Uint32Array; count: Uint32Array };
  membrane?: { neuron: Uint32Array; offset: Uint32Array; mv: Float32Array };
  weights?: { post: Uint32Array; synapse: Uint32Array; weight: Float32Array };
  transitions?: { neuron: Uint32Array; offset: Uint32Array; tonic: Uint8Array };
}

/** The simulation time of an offset in a frame, ms. */
export function timeMs(header: FrameHeader, offset: number): number {
  return header.t0Ms + (offset * header.timeUnitNs) / 1e6;
}

/** Total spikes in a frame, whatever its level of detail. */
export function spikeCount(frame: Frame): number {
  let n = frame.spikes?.neuron.length ?? 0;
  for (const c of frame.spikeCounts?.count ?? []) n += c;
  for (const c of frame.spikeBins?.count ?? []) n += c;
  return n;
}

export class FrameError extends Error {}

/** Decodes a frame. `data` should start on an 8-byte boundary (a WebSocket's ArrayBuffer does); otherwise it's copied. */
export function decodeFrame(data: ArrayBuffer | Uint8Array): Frame {
  let buffer: ArrayBuffer;
  let base: number;
  let length: number;
  if (data instanceof Uint8Array) {
    if (data.byteOffset % 8 === 0 && data.buffer instanceof ArrayBuffer) {
      buffer = data.buffer;
      base = data.byteOffset;
    } else {
      buffer = data.slice().buffer as ArrayBuffer;
      base = 0;
    }
    length = data.byteLength;
  } else {
    buffer = data;
    base = 0;
    length = data.byteLength;
  }
  if (length < HEADER_BYTES) throw new FrameError("too short for a frame");
  const view = new DataView(buffer, base, length);
  const magic = String.fromCharCode(view.getUint8(0), view.getUint8(1), view.getUint8(2), view.getUint8(3));
  if (magic !== MAGIC) throw new FrameError("not a frame");
  const t0Ns = view.getBigUint64(16, true);
  const t1Ns = view.getBigUint64(24, true);
  const header: FrameHeader = {
    version: view.getUint16(4, true),
    session: view.getUint32(8, true),
    seq: view.getUint32(12, true),
    viewer: view.getUint16(50, true),
    t0Ns,
    t1Ns,
    t0Ms: Number(t0Ns) / 1e6,
    t1Ms: Number(t1Ns) / 1e6,
    wallUnixMs: Number(view.getBigUint64(32, true)),
    speed: view.getFloat32(40, true),
    lagMs: view.getFloat32(44, true),
    lod: view.getUint8(48) as Lod,
    droppedEvents: view.getUint32(52, true),
    droppedFrames: view.getUint32(56, true),
    timeUnitNs: view.getUint32(60, true),
  };
  const frame: Frame = { header };
  const sections = view.getUint8(49);
  let at = HEADER_BYTES;
  for (let s = 0; s < sections; s++) {
    if (at + SECTION_HEADER_BYTES > length) throw new FrameError("truncated section header");
    const kind = view.getUint16(at, true);
    const count = view.getUint32(at + 4, true);
    const bytes = view.getUint32(at + 8, true);
    let column = at + SECTION_HEADER_BYTES;
    const end = column + bytes;
    if (end > length) throw new FrameError(`section ${kind} overruns the frame`);
    const take = <T>(make: (b: ArrayBuffer, o: number, n: number) => T, width: number): T => {
      const value = make(buffer, base + column, count);
      column += Math.ceil((count * width) / 8) * 8;
      if (column > end) throw new FrameError(`section ${kind} is shorter than its columns`);
      return value;
    };
    const u32 = () => take((b, o, n) => new Uint32Array(b, o, n), 4);
    const u16 = () => take((b, o, n) => new Uint16Array(b, o, n), 2);
    const u8 = () => take((b, o, n) => new Uint8Array(b, o, n), 1);
    const f32 = () => take((b, o, n) => new Float32Array(b, o, n), 4);
    switch (kind) {
      case 1:
        frame.spikes = { neuron: u32(), offset: u32() };
        break;
      case 2:
        frame.spikeCounts = { neuron: u32(), count: u16() };
        break;
      case 3:
        frame.spikeBins = { cell: u32(), count: u32() };
        break;
      case 4:
        frame.membrane = { neuron: u32(), offset: u32(), mv: f32() };
        break;
      case 5:
        frame.weights = { post: u32(), synapse: u32(), weight: f32() };
        break;
      case 6:
        frame.transitions = { neuron: u32(), offset: u32(), tonic: u8() };
        break;
      default:
        break; // a newer section kind: skip it
    }
    at = end;
  }
  return frame;
}
