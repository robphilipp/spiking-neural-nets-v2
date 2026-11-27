// Data files for formulas' `data("name", t, c)` (app plan 05 §3): a CSV table or a WAV recording, read into channels
// of samples that a formula looks up in time with linear interpolation. The same parsers and lookups as the engine's
// (crates/spikes-signal/src/data.rs); test/signal.test.ts checks them on the same bytes.
//
//  - CSV: comma, semicolon or tab separated (the first line decides); an optional header row (a first row with a cell
//    that isn't a number); blank lines and lines starting with `#` are skipped. The first column is time, in ms unless
//    the header's first cell is `t_s` or `time_s` (seconds); every other column is a channel. Numbers only; times
//    strictly ascending; every row as wide as the first.
//  - WAV: RIFF/WAVE, PCM 8 (unsigned), 16, 24 or 32 bit integers, or 32 or 64 bit floats; any number of channels;
//    time is the sample's index over the sample rate from the header. Integers are scaled to [−1, 1).
//
// A lookup is 0 before the first sample and after the last, and for a channel the file doesn't have. Values are kept
// as 32-bit floats (as in the engine).

export class DataFileError extends Error {}

export interface DataSet {
  /** Per channel, the samples. */
  channels: Float32Array[];
  /** ms per row (CSV); null for a WAV. */
  times: Float64Array | null;
  /** Samples per second (WAV); null for a CSV. */
  rateHz: number | null;
  /** The number of samples (rows) per channel. */
  length: number;
  /** The time of the first sample, ms. */
  startMs: number;
  /** The time of the last sample, ms. */
  durationMs: number;
}

/** The data sets formulas can look up, by name. */
export type DataSets = ReadonlyMap<string, DataSet>;

/** Reads `bytes` as WAV if they start with `RIFF`, else as CSV; `name` is for the errors. */
export function parseData(name: string, bytes: Uint8Array): DataSet {
  const fail = (m: string): never => {
    throw new DataFileError(`data file '${name}': ${m}`);
  };
  const isWav = bytes.length >= 4 && String.fromCharCode(bytes[0]!, bytes[1]!, bytes[2]!, bytes[3]!) === "RIFF";
  return isWav ? parseWav(bytes, fail) : parseCsv(bytes, fail);
}

const NUMBER = /^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$/;

function number(s: string): number | null {
  if (!NUMBER.test(s)) return null;
  const v = Number(s);
  return Number.isFinite(v) ? v : null;
}

function parseCsv(bytes: Uint8Array, fail: (m: string) => never): DataSet {
  let text: string;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return fail("not text (is it a CSV or a WAV file?)");
  }
  if (text.startsWith("﻿")) text = text.slice(1);
  const lines = text
    .split(/\r\n|\n|\r/)
    .map((l, i) => ({ no: i + 1, text: l.trim() }))
    .filter((l) => l.text !== "" && !l.text.startsWith("#"));
  if (!lines.length) return fail("it has no rows");
  const first = lines[0]!.text;
  const delimiter = first.includes("\t") ? "\t" : first.includes(";") ? ";" : ",";
  let seconds = false;
  const header = first.split(delimiter).map((c) => c.trim());
  if (header.some((c) => number(c) === null)) {
    seconds = header[0] === "t_s" || header[0] === "time_s";
    lines.shift();
  }
  const times: number[] = [];
  let channels: number[][] = [];
  for (const line of lines) {
    const cells = line.text.split(delimiter).map((c) => c.trim());
    if (cells.length < 2) return fail(`line ${line.no}: needs a time column and at least one channel column`);
    if (!channels.length) channels = Array.from({ length: cells.length - 1 }, () => []);
    else if (cells.length !== channels.length + 1) {
      return fail(`line ${line.no}: has ${cells.length} columns, the first row has ${channels.length + 1}`);
    }
    const values = cells.map((cell, col) => {
      const v = number(cell);
      return v === null ? fail(`line ${line.no}, column ${col + 1}: '${cell}' is not a number`) : v;
    });
    const t = seconds ? values[0]! * 1000 : values[0]!;
    if (times.length && t <= times[times.length - 1]!) return fail(`line ${line.no}: times must be ascending`);
    times.push(t);
    for (let c = 0; c < channels.length; c++) channels[c]!.push(values[c + 1]!);
  }
  if (!times.length) return fail("it has no rows");
  return {
    channels: channels.map((c) => Float32Array.from(c)),
    times: Float64Array.from(times),
    rateHz: null,
    length: times.length,
    startMs: times[0]!,
    durationMs: times[times.length - 1]!,
  };
}

function parseWav(b: Uint8Array, fail: (m: string) => never): DataSet {
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const tag4 = (i: number) => String.fromCharCode(b[i]!, b[i + 1]!, b[i + 2]!, b[i + 3]!);
  if (b.length < 12 || tag4(8) !== "WAVE") return fail("not a WAV file (no RIFF/WAVE header)");
  let format: { tag: number; channels: number; rate: number; bits: number } | null = null;
  let data: { from: number; to: number } | null = null;
  let i = 12;
  while (i + 8 <= b.length) {
    const id = tag4(i);
    const size = view.getUint32(i + 4, true);
    const body = i + 8;
    const end = Math.min(body + size, b.length);
    if (id === "fmt ") {
      if (end - body < 16) return fail("the fmt chunk is too short");
      let tag = view.getUint16(body, true);
      if (tag === 0xfffe && end - body >= 26) tag = view.getUint16(body + 24, true);
      format = { tag, channels: view.getUint16(body + 2, true), rate: view.getUint32(body + 4, true), bits: view.getUint16(body + 14, true) };
    } else if (id === "data") {
      data = { from: body, to: end };
    }
    i = body + size + (size & 1);
  }
  if (!format) return fail("there is no fmt chunk");
  if (!data) return fail("there is no data chunk");
  const { tag, channels, rate, bits } = format;
  if (channels === 0 || rate === 0) return fail("the header has no channels or no sample rate");
  const float = tag === 3 && (bits === 32 || bits === 64);
  const int = tag === 1 && (bits === 8 || bits === 16 || bits === 24 || bits === 32);
  if (!float && !int) {
    return fail(`unsupported format (code ${tag}, ${bits} bits): PCM 8, 16, 24 or 32 bit integers, or 32 or 64 bit floats`);
  }
  const width = bits / 8;
  const frames = Math.floor((data.to - data.from) / (width * channels));
  if (frames === 0) return fail("it has no samples");
  const out = Array.from({ length: channels }, () => new Float32Array(frames));
  for (let f = 0; f < frames; f++) {
    for (let c = 0; c < channels; c++) {
      const p = data.from + (f * channels + c) * width;
      let v: number;
      if (float) v = bits === 32 ? view.getFloat32(p, true) : view.getFloat64(p, true);
      else if (bits === 8) v = (view.getUint8(p) - 128) / 128;
      else if (bits === 16) v = view.getInt16(p, true) / 32768;
      else if (bits === 24) v = ((view.getInt8(p + 2) << 16) | (view.getUint8(p + 1) << 8) | view.getUint8(p)) / 8388608;
      else v = view.getInt32(p, true) / 2147483648;
      out[c]![f] = Number.isFinite(v) ? v : 0;
    }
  }
  return { channels: out, times: null, rateHz: rate, length: frames, startMs: 0, durationMs: ((frames - 1) * 1000) / rate };
}

/** The value of channel `c` at `tMs`, linearly interpolated; 0 outside the samples' time, for a channel the file
 *  doesn't have, and for a time that isn't finite. */
export function lookup(d: DataSet, tMs: number, c: number): number {
  const values = d.channels[c];
  if (!values || !Number.isInteger(c) || c < 0 || !values.length || !Number.isFinite(tMs)) return 0;
  const last = values.length - 1;
  const { times } = d;
  if (times) {
    // The count of times ≤ tMs.
    let lo = 0;
    let hi = times.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (times[mid]! <= tMs) lo = mid + 1;
      else hi = mid;
    }
    if (lo === 0) return 0;
    if (lo > last) return tMs === times[last] ? values[last]! : 0;
    const [t0, t1, v0, v1] = [times[lo - 1]!, times[lo]!, values[lo - 1]!, values[lo]!];
    return v0 + (v1 - v0) * ((tMs - t0) / (t1 - t0));
  }
  const pos = (tMs * d.rateHz!) / 1000;
  if (pos < 0 || pos > last) return 0;
  const i = Math.floor(pos);
  if (i >= last) return values[last]!;
  const [v0, v1] = [values[i]!, values[i + 1]!];
  return v0 + (v1 - v0) * (pos - i);
}

/** Bytes as base64 (how data files travel in `setInputs`). */
export function toBase64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

/** Base64 → bytes. */
export function fromBase64(text: string): Uint8Array {
  return Uint8Array.from(atob(text), (ch) => ch.charCodeAt(0));
}
