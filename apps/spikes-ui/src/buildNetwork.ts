import type { Network, ProtocolClient } from "@spikes/client";
import { engineSources, expandTrials, type InputFile } from "./inputFile";
import { built } from "./store";

/** Builds the session's network (on `nodes`, or the engine alone when empty), gives the run the input file's
 *  sources (trials expanded with the seed, targets resolved in the built network; nothing for none) and trials,
 *  and makes it the session's network. */
export async function buildNetwork(
  client: ProtocolClient,
  id: number,
  { shards, seed, nodes, inputs }: { shards: number; seed: number; nodes: string[]; inputs: InputFile | null },
): Promise<Network> {
  await client.build(id, { shards, seed, nodes });
  const net = await client.getNetwork(id);
  if (inputs) {
    // Trials are expanded here, with the run's seed (the same file and seed give the same trials).
    const { file, trials } = expandTrials(inputs, seed);
    const sources = engineSources(file, net);
    if (sources.length || trials.length) await client.setInputs(id, sources, trials);
  }
  built(net);
  return net;
}
