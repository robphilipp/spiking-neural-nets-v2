// Opening a project ready to run: apply its network to the engine and build it with the selected input file and the
// project's run settings, as the Network page's Apply and the Run page's Build do.

import { type ProtocolClient, SpikesError } from "@spikes/client";
import { buildNetwork, inputsKey } from "./buildNetwork";
import { emptyInputFile } from "./inputFile";
import { applied, parseFailed, useProject, useSession, useSettings } from "./store";

/** The network has errors: nothing was applied (they're marked in the editor). */
export class NetworkErrors extends Error {}

/** Applies the network in the editor (a new session, or the current one's) and builds it. Throws `NetworkErrors` for
 *  a description that doesn't parse; other errors (building, the input file) as they come. */
export async function applyAndBuild(client: ProtocolClient): Promise<void> {
  const { id, name, description } = useSession.getState();
  let session = id;
  try {
    if (session === null) {
      session = (await client.createSession(name, description)).id;
    } else {
      await client.updateDescription(session, description);
    }
    applied({ id: session, description });
  } catch (e) {
    if (e instanceof SpikesError && e.error.code === "parse") {
      const spans = e.error.spans ?? [];
      parseFailed(spans);
      throw new NetworkErrors(`${spans.length} error${spans.length === 1 ? "" : "s"} in the network description: fix ${spans.length === 1 ? "it" : "them"}, then apply`);
    }
    throw e;
  }
  const { project, inputFiles, dataFiles } = useProject.getState();
  const name0 = project.selectedInputs;
  const inputs = name0 === null ? null : (inputFiles[name0] ?? emptyInputFile());
  const nodes = useSettings.getState().nodes.split(",").map((n) => n.trim()).filter(Boolean);
  await buildNetwork(client, session, {
    shards: project.run.shards,
    seed: project.run.seed,
    nodes,
    inputs,
    inputsKey: inputsKey(name0, inputs),
    dataFiles,
  });
}
