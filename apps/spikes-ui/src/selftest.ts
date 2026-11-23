// Desktop self-test (SPIKES_SELFTEST=1): runs the ring network through the built-in engine over Tauri IPC and
// reports to the app, which prints the result and exits. Checks JSON requests, notices and raw-byte frames.

import { type Frame, spikeCount } from "@spikes/client";
import { invoke } from "@tauri-apps/api/core";
import { examples } from "./examples";
import { TauriClient } from "./tauriClient";

export async function selftest(): Promise<void> {
  const report = (ok: boolean, text: string) => invoke("selftest_report", { ok, report: text });
  try {
    const client = await TauriClient.connect();
    const ring = examples.find((e) => e.name === "ring")!.text;
    const { id } = await client.createSession("selftest", ring);
    await client.build(id, { shards: 2 });
    const network = await client.getNetwork(id);
    const frames: Frame[] = [];
    await client.attach(id, { frameMs: 16, budgetBytesPerS: 4 << 20, spikes: { type: "all" }, membrane: [], weights: { type: "nothing" } }, (f) => frames.push(f));
    let ended = false;
    client.onNotice((_, n) => {
      if (n.type === "runEnded") ended = true;
    });
    const group1 = network.neurons.filter((n) => n.name.startsWith("hidden-10")).map((n) => n.index);
    await client.stimulate(id, group1, 10);
    await client.start(id, { durationMs: 200 });
    const deadline = performance.now() + 5000;
    while ((!ended || frames.reduce((n, f) => n + spikeCount(f), 0) < 25) && performance.now() < deadline) {
      await new Promise((r) => setTimeout(r, 20));
    }
    const spikes = frames.reduce((n, f) => n + spikeCount(f), 0);
    const ok = ended && spikes === 25 && network.neurons.length === 25;
    await report(ok, `neurons=${network.neurons.length} frames=${frames.length} spikes=${spikes} runEnded=${ended}`);
  } catch (e) {
    await report(false, `error: ${e instanceof Error ? e.message : String(e)}`);
  }
}
