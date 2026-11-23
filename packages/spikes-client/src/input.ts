// Sensor input batches, client → engine (see crates/spikes-api/src/input.rs for the layout).

const HEADER_BYTES = 32;
export const PROTOCOL_VERSION = 1;

function header(view: DataView, flags: number, sensor: number, seq: number, t0Ns: bigint, channels: number, count: number, dtNs: number) {
  view.setUint8(0, 0x53); // S
  view.setUint8(1, 0x50); // P
  view.setUint8(2, 0x4b); // K
  view.setUint8(3, 0x53); // S
  view.setUint16(4, PROTOCOL_VERSION, true);
  view.setUint16(6, flags, true);
  view.setUint32(8, sensor, true);
  view.setUint32(12, seq, true);
  view.setBigUint64(16, t0Ns, true);
  view.setUint16(24, channels, true);
  view.setUint16(26, count, true);
  view.setUint32(28, dtNs, true);
}

/** Samples on all channels at a fixed spacing; `values` is sample-major (`values[s * channels + c]`). */
export function encodeSamples(sensor: number, seq: number, t0Ns: bigint, dtNs: number, channels: number, values: ArrayLike<number>): ArrayBuffer {
  if (channels <= 0 || values.length % channels !== 0) {
    throw new RangeError(`${values.length} values aren't whole samples of ${channels} channels`);
  }
  const count = values.length / channels;
  if (count > 0xffff) throw new RangeError("at most 65535 samples per batch");
  const buffer = new ArrayBuffer(HEADER_BYTES + values.length * 4);
  const view = new DataView(buffer);
  header(view, 0, sensor, seq, t0Ns, channels, count, dtNs);
  new Float32Array(buffer, HEADER_BYTES).set(values);
  return buffer;
}

/** Events from a sensor that produces spikes: channel and offset (ns from `t0Ns`) per event. */
export function encodeEvents(sensor: number, seq: number, t0Ns: bigint, channels: ArrayLike<number>, offsetsNs: ArrayLike<number>): ArrayBuffer {
  const count = channels.length;
  if (offsetsNs.length !== count) throw new RangeError("one offset per event");
  if (count > 0xffff) throw new RangeError("at most 65535 events per batch");
  const channelBytes = Math.ceil((count * 2) / 4) * 4;
  const buffer = new ArrayBuffer(HEADER_BYTES + channelBytes + count * 4);
  const view = new DataView(buffer);
  header(view, 1, sensor, seq, t0Ns, 0, count, 0);
  new Uint16Array(buffer, HEADER_BYTES, count).set(channels);
  new Uint32Array(buffer, HEADER_BYTES + channelBytes, count).set(offsetsNs);
  return buffer;
}
