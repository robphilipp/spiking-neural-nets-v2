// What a source's channels drive in a built network (position mapping, see channelLayout.ts): the target neurons its
// `neurons` match, and the neurons each channel gets.

import type { Network } from "@spikes/client";
import { type ChannelResult, hasChannels, channelTargets } from "./channelLayout";
import { type LoadedData } from "./inputData";
import { type Source, targetNeurons } from "./inputFile";

export interface SourceChannels {
  /** The target neurons (network order). */
  targets: number[];
  /** The channel mapping; null for a source without channels (pulse, poisson, periodic, sensor). */
  result: ChannelResult | null;
}

/** The source's targets and channel mapping in `network`; `{ error }` if its targets can't be resolved (an unknown set,
 *  a bad regular expression). `data`: the loaded data files (a file source's channels are the file's). */
export function sourceChannels(s: Source, network: Network, data: LoadedData | null): SourceChannels | { error: string } {
  let targets: number[];
  try {
    targets = targetNeurons(s.targets, network);
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
  if (!hasChannels(s)) return { targets, result: null };
  if (s.kind === "file") {
    const set = data?.sets.get(s.data);
    if (!set) return { targets, result: { ok: false, error: data?.errors.get(s.data) ?? `the data '${s.data}' isn't in the file's [[data]]` } };
    return { targets, result: channelTargets(s, network, targets, set.channels.length) };
  }
  return { targets, result: channelTargets(s, network, targets) };
}

/** The mapping's warnings in a line ("3 channels get no neuron; 2 target neurons are ignored"), or null. */
export function mappingWarning(r: Extract<ChannelResult, { ok: true }>): string | null {
  const parts = [
    r.empty ? `${r.empty} of ${r.channels.length} channel${r.channels.length === 1 ? "" : "s"} get${r.empty === 1 ? "s" : ""} no neuron` : "",
    r.ignored ? `${r.ignored} target neuron${r.ignored === 1 ? " is" : "s are"} ignored` : "",
  ].filter(Boolean);
  return parts.length ? parts.join("; ") : null;
}
