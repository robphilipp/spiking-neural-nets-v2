// Desktop self-test (SPIKES_SELFTEST=1): runs the ring network through the built-in engine over Tauri IPC and
// reports to the app, which prints the result and exits. Checks JSON requests, notices, raw-byte frames, and sensor
// samples sent as raw bytes (the Inputs page's path).

import { type Frame, spikeCount } from "@spikes/client";
import { invoke } from "@tauri-apps/api/core";
import { appDataDir, join } from "@tauri-apps/api/path";
import { mkdir, writeTextFile } from "@tauri-apps/plugin-fs";
import { examples } from "./examples";
import { readProject, writeProject } from "./files";
import { parseInputFile } from "./inputFile";
import { newProject } from "./project";
import { LiveView } from "./live";
import { TauriClient } from "./tauriClient";

export async function selftest(): Promise<void> {
  const report = (ok: boolean, text: string) =>
    invoke("selftest_report", { ok, report: text });
  try {
    // Project files through the real fs plugin and its permissions (the app's data folder is in scope).
    const folder = await join(await appDataDir(), "selftest-project");
    const project = newProject("selftest");
    const inputFiles = {
      [project.inputs[0]!]: parseInputFile(
        '[[source]]\nname = "background"\nkind = "poisson"\nneurons = { prefix = "" }\nhz = 5\nmv = 12\n',
      ),
    };
    await writeProject(
      folder,
      project,
      "(neurons=[(name=a, position=[0, 0, 0])])",
      inputFiles,
    );
    const back = await readProject(folder);
    // A folder of networks without project.toml: one is the project's network; with several, which is asked.
    // (Two folders whose contents never change: files written by an earlier run are still there, and the app may
    // not delete files.)
    const loose = await join(await appDataDir(), "selftest-one-network");
    await mkdir(loose, { recursive: true });
    await writeTextFile(await join(loose, "one.boo"), "(neurons=[(name=one, position=[0, 0, 0])])");
    const single = await readProject(loose);
    const looser = await join(await appDataDir(), "selftest-two-networks");
    await mkdir(looser, { recursive: true });
    await writeTextFile(await join(looser, "one.boo"), "(neurons=[(name=one, position=[0, 0, 0])])");
    await writeTextFile(await join(looser, "two.boo"), "(neurons=[(name=two, position=[0, 0, 0])])");
    const several = await readProject(looser);
    const chosen = await readProject(looser, "two.boo");
    // Each check by name, so a failure says which.
    const projectChecks: Record<string, boolean> = {
      read: back.kind === "project",
      saved: back.kind === "project" && back.saved,
      settings: back.kind === "project" && JSON.stringify(back.project) === JSON.stringify(project),
      inputFiles: back.kind === "project" && JSON.stringify(back.inputFiles) === JSON.stringify(inputFiles),
      network: back.kind === "project" && back.description.includes("name=a"),
      singleNetwork:
        single.kind === "project" && !single.saved && single.project.network === "one.boo" && single.description.includes("name=one"),
      severalNetworks: several.kind === "choose" && several.networks.join() === "one.boo,two.boo",
      chosenNetwork: chosen.kind === "project" && chosen.description.includes("name=two"),
    };
    const projectFailed = Object.entries(projectChecks).filter(([, ok]) => !ok).map(([name]) => name);
    const projectOk = projectFailed.length === 0;
    const projectNote = projectOk
      ? ""
      : ` projectFailed=${projectFailed.join(",")}${back.kind === "project" ? ` readBack=${JSON.stringify(back.project)} ${JSON.stringify(back.inputFiles)}` : ""}`;

    const client = await TauriClient.connect();
    const ring = examples.find((e) => e.name === "ring")!.text;
    const { id } = await client.createSession("selftest", ring);
    await client.build(id, { shards: 2 });
    const network = await client.getNetwork(id);
    const frames: Frame[] = [];
    await client.attach(
      id,
      {
        frameMs: 16,
        budgetBytesPerS: 4 << 20,
        spikes: { type: "all" },
        membrane: [],
        weights: { type: "nothing" },
      },
      (f) => frames.push(f),
    );
    let ended = false;
    client.onNotice((_, n) => {
      if (n.type === "runEnded") ended = true;
    });
    // The charts' data path too: a LiveView, as the Run page uses.
    const live = await LiveView.attach(client, id, network.neurons.length);
    await live.select(network.neurons);
    const group1 = network.neurons
      .filter((n) => n.name.startsWith("hidden-10"))
      .map((n) => n.index);
    await client.stimulate(id, group1, 10);
    await client.start(id, { durationMs: 200 });
    const deadline = performance.now() + 5000;
    while (
      (!ended || frames.reduce((n, f) => n + spikeCount(f), 0) < 25) &&
      performance.now() < deadline
    ) {
      await new Promise((r) => setTimeout(r, 20));
    }
    const spikes = frames.reduce((n, f) => n + spikeCount(f), 0);
    await new Promise((r) => setTimeout(r, 300));
    const rasterPoints = live.raster.value
      .seriesList()
      .reduce((n, s) => n + s.length(), 0);
    const ratePoints = live.rate
      .seriesList()
      .reduce((n, s) => n + s.length(), 0);
    await live.dispose();

    // Sensor input over the binary IPC path, as the Inputs page injects it: wta-learning, its inputs driven.
    const wta = examples.find((e) => e.name === "wta-learning")!.text;
    const { id: w } = await client.createSession("selftest-wta", wta);
    await client.build(w, { shards: 1 });
    const wtaNet = await client.getNetwork(w);
    const inputs = wtaNet.neurons
      .filter((n) => n.name.startsWith("in-"))
      .map((n) => [n.index]);
    const sensor = await client.connectSensor(w, {
      name: "designed signal",
      targets: inputs,
      encoding: { type: "rate", maxHz: 100, mv: 10 },
      bufferWhilePaused: true,
      ingressDelayMs: 1,
    });
    const errors: string[] = [];
    const consoleError = console.error;
    console.error = (...a: unknown[]) => errors.push(a.map(String).join(" "));
    const steps = 2000;
    const values = new Float32Array(steps * 2);
    for (let t = 0; t < steps; t++)
      values[t * 2 + (Math.floor(t / 300) % 2)] = 1;
    sensor.sendSamples(0n, 1e6, 2, values);
    await new Promise((r) => setTimeout(r, 300));
    let wtaEnded = false;
    client.onNotice((s, n) => {
      if (s === w && n.type === "runEnded") wtaEnded = true;
    });
    await client.start(w, { durationMs: 2000 });
    const wtaDeadline = performance.now() + 5000;
    while (!wtaEnded && performance.now() < wtaDeadline)
      await new Promise((r) => setTimeout(r, 20));
    let inputSpikes = 0;
    for (const [i] of inputs)
      inputSpikes += (await client.queryNeuron(w, i!)).spikes;
    console.error = consoleError;
    const sensorsOk = inputSpikes > 50 && errors.length === 0;

    // A malformed batch: the engine's rejection comes back through the IPC and reaches onError (the page's banner).
    const reported: string[] = [];
    const unsubscribe = client.onError((e) => reported.push(e.message));
    (client as unknown as { sendBinary(data: ArrayBuffer): void }).sendBinary(
      new Uint8Array([1, 2, 3]).buffer,
    );
    const errorDeadline = performance.now() + 2000;
    while (reported.length === 0 && performance.now() < errorDeadline)
      await new Promise((r) => setTimeout(r, 20));
    unsubscribe();
    const errorsOk = reported.length === 1;

    const ok =
      rasterPoints === 25 &&
      ratePoints > 0 &&
      projectOk &&
      ended &&
      spikes === 25 &&
      network.neurons.length === 25 &&
      sensorsOk &&
      errorsOk;
    await report(
      ok,
      `raster=${rasterPoints} rate=${ratePoints} project=${projectOk}${projectNote} neurons=${network.neurons.length} frames=${frames.length} spikes=${spikes} runEnded=${ended} sensorInputSpikes=${inputSpikes} errors=${errors.join(" | ")} reported=${reported.join(" | ")}`,
    );
  } catch (e) {
    await report(false, `error: ${e instanceof Error ? e.message : String(e)}`);
  }
}
