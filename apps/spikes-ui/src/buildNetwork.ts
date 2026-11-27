import type { Network, ProtocolClient } from "@spikes/client";
import { engineSources, expandTrials, type InputFile } from "./inputFile";
import { built } from "./store";

/** Builds the session's network (on `nodes`, or the engine alone when empty), gives the run the input file's
 *  sources (trials expanded with the seed, targets resolved in the built network; nothing for none) and trials,
 *  and makes it the session's network. `inputsKey` says which input file that was (see `inputsKey`). */
export async function buildNetwork(
  client: ProtocolClient,
  id: number,
  { shards, seed, nodes, inputs, inputsKey }: { shards: number; seed: number; nodes: string[]; inputs: InputFile | null; inputsKey: string },
): Promise<Network> {
  await client.build(id, { shards, seed, nodes });
  const net = await client.getNetwork(id);
  if (inputs) {
    // Trials are expanded here, with the run's seed (the same file and seed give the same trials).
    const { file, trials } = expandTrials(inputs, seed);
    const sources = engineSources(file, net);
    if (sources.length || trials.length) await client.setInputs(id, sources, trials);
  }
  built(net, inputsKey);
  return net;
}

/** Names the input file a build was given (its name and contents), to tell later whether it has changed. */
export const inputsKey = (name: string | null, file: InputFile | null): string => JSON.stringify([name, file]);

/** Whether the input file has changed since the network was built with `builtInputs` (null: not known, as for a
 *  session opened as it was). */
export const inputsChanged = (builtInputs: string | null, name: string | null, file: InputFile | null): boolean =>
  builtInputs !== null && builtInputs !== inputsKey(name, file);
