import type { Network, ProtocolClient } from "@spikes/client";
import { engineSources, type InputFile } from "./inputFile";
import { built } from "./store";

/** Builds the session's network (on `nodes`, or the engine alone when empty), gives the run the input file's
 *  sources (resolved in the built network; nothing for none), and makes it the session's network. */
export async function buildNetwork(
  client: ProtocolClient,
  id: number,
  { shards, seed, nodes, inputs }: { shards: number; seed: number; nodes: string[]; inputs: InputFile | null },
): Promise<Network> {
  await client.build(id, { shards, seed, nodes });
  const net = await client.getNetwork(id);
  if (inputs) {
    const sources = engineSources(inputs, net);
    if (sources.length) await client.setInputs(id, sources);
  }
  built(net);
  return net;
}
