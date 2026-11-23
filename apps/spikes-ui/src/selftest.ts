// Desktop self-test (SPIKES_SELFTEST=1): runs the ring network through the built-in engine over Tauri IPC and
// reports to the app, which prints the result and exits. Checks JSON requests, notices and raw-byte frames.

import { type Frame, spikeCount } from "@spikes/client";
import { invoke } from "@tauri-apps/api/core";
import { appDataDir, join } from "@tauri-apps/api/path";
import { examples } from "./examples";
import { readProject, writeProject } from "./files";
import { newProject } from "./project";
import { TauriClient } from "./tauriClient";

export async function selftest(): Promise<void> {
  const report = (ok: boolean, text: string) => invoke("selftest_report", { ok, report: text });
  try {
    // Project files through the real fs plugin and its permissions (the app's data folder is in scope).
    const folder = await join(await appDataDir(), "selftest-project");
    const project = { ...newProject("selftest"), background: { rateHz: 5, mv: 12 } };
    await writeProject(folder, project, "(NRN=[(nid=a, LOC=(px1=0, px2=0, px3=0))])");
    const back = await readProject(folder);
    const projectOk = JSON.stringify(back.project) === JSON.stringify(project) && back.description.includes("nid=a");

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
    const ok = projectOk && ended && spikes === 25 && network.neurons.length === 25;
    await report(ok, `project=${projectOk} neurons=${network.neurons.length} frames=${frames.length} spikes=${spikes} runEnded=${ended}`);
  } catch (e) {
    await report(false, `error: ${e instanceof Error ? e.message : String(e)}`);
  }
}
