import type { Network, ProtocolClient } from "@spikes/client";
import type { Background } from "./project";
import { built } from "./store";

/** Builds the session's network (on `nodes`, or the engine alone when empty), adds the background input, and
 *  makes it the session's network. */
export async function buildNetwork(
  client: ProtocolClient,
  id: number,
  { shards, seed, nodes, background }: { shards: number; seed: number; nodes: string[]; background: Background | null },
): Promise<Network> {
  await client.build(id, { shards, seed, nodes });
  const net = await client.getNetwork(id);
  if (background) {
    const targets = net.neurons.filter((n) => n.name.startsWith(background.prefix)).map((n) => n.index);
    if (!targets.length) throw new Error(`background input: no neuron names start with '${background.prefix}'`);
    await client.addInput(id, { type: "poisson", targets, rateHz: background.rateHz * targets.length, mv: background.mv, durationMs: null });
  }
  built(net);
  return net;
}
