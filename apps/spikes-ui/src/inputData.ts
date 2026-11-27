// The data files an input file refers to (`[[data]]`): their bytes (kept in the project store, read from the project
// folder or opened in the browser), parsed for previews and checks, and given to the engine with the inputs.

import type { DataFile } from "@spikes/client";
import { type DataSet, DataFileError, parseData, toBase64 } from "./dataFile";
import { dataNamesOf, type InputFile } from "./inputFile";

/** The largest data the engine takes in one `setInputs`, bytes: its WebSocket messages are at most 64 MiB, and base64
 *  makes files a third larger. */
export const MAX_DATA_BYTES = 40 * 1024 * 1024;

export interface LoadedData {
  /** The data sets that read, by their `[[data]]` name. */
  sets: Map<string, DataSet>;
  /** Why a data file can't be used, by name: not loaded (missing), or not readable. */
  errors: Map<string, string>;
}

/** Parses the input file's data files from `bytes` (by path in the project folder). */
export function loadData(file: InputFile | null, bytes: Record<string, Uint8Array>): LoadedData {
  const sets = new Map<string, DataSet>();
  const errors = new Map<string, string>();
  for (const ref of file?.data ?? []) {
    const b = bytes[ref.file];
    if (!b) {
      errors.set(ref.name, `data file '${ref.name}': ${ref.file} isn't loaded (it should be in the project folder)`);
      continue;
    }
    try {
      sets.set(ref.name, parseData(ref.name, b));
    } catch (e) {
      if (!(e instanceof DataFileError)) throw e;
      errors.set(ref.name, e.message);
    }
  }
  return { sets, errors };
}

/** The data files the (expanded) input file's enabled sources read, for `setInputs`. Throws an Error for too much. */
export function requestData(file: InputFile, bytes: Record<string, Uint8Array>): DataFile[] {
  const used = new Set(file.sources.filter((s) => s.enabled).flatMap(dataNamesOf));
  const out = (file.data ?? []).filter((d) => used.has(d.name) && bytes[d.file]).map((d) => ({ name: d.name, base64: toBase64(bytes[d.file]!) , size: bytes[d.file]!.length }));
  const total = out.reduce((n, d) => n + d.size, 0);
  if (total > MAX_DATA_BYTES) {
    throw new Error(`the data files are ${(total / 1048576).toFixed(0)} MB; the engine takes at most ${MAX_DATA_BYTES / 1048576} MB of data per run`);
  }
  return out.map(({ name, base64 }) => ({ name, base64 }));
}
