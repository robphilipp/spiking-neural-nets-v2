// Test runs (app plan 05, I5): the pure parts. Which groups of neurons to count, the inputs a test run gets, and what
// the engine's per-bin counts become: rates, and the response aligned to the trials, averaged per condition.

import type { InputSource, Network, TestGroup, TestRunResult } from "@spikes/client";
import { engineSources, expandTrials, isRealtime, SIMULATED, targetNeurons, type InputFile, type Trial } from "./inputFile";
import { describeMatch, matchNeurons, type NeuronMatch } from "./neuronMatch";

/** A test run's default length without trials, ms. */
export const DEFAULT_DURATION_MS = 5000;
/** The longest test run the panel asks for, ms. */
export const MAX_DURATION_MS = 60_000;
/** The default bin width, ms. */
export const DEFAULT_BIN_MS = 50;

/** How long to run by default: to the end of the last trial (up to the cap), else 5 s. */
export function defaultDurationMs(trials: Trial[]): number {
  if (trials.length === 0) return DEFAULT_DURATION_MS;
  const end = Math.ceil(Math.max(...trials.map((t) => t.endMs)));
  return Math.min(MAX_DURATION_MS, Math.max(1, end));
}

/** Which groups to count. */
export interface GroupSelection {
  /** One group per enabled simulated source: its targets, named after it. */
  sources: boolean;
  /** Sets of the network, by name. */
  sets: string[];
  /** The whole network. */
  all: boolean;
  /** Neurons picked by name. */
  custom: NeuronMatch[];
}

export const emptySelection = (): GroupSelection => ({ sources: true, sets: [], all: false, custom: [] });

/** A custom group's name: what it matches. */
export const customName = (m: NeuronMatch) => (m.mode === "prefix" ? `starts with ${m.pattern}` : `/${m.pattern}/`);

/** The groups a selection means in `network`, with names made unique, and what couldn't be resolved. */
export function resolveGroups(selection: GroupSelection, file: InputFile, network: Network): { groups: TestGroup[]; problems: string[] } {
  const groups: TestGroup[] = [];
  const problems: string[] = [];
  const used = new Set<string>();
  const add = (name: string, neurons: number[]) => {
    let unique = name;
    for (let k = 2; used.has(unique); k++) unique = `${name} (${k})`;
    used.add(unique);
    groups.push({ name: unique, neurons });
  };
  if (selection.sources) {
    for (const s of file.sources) {
      if (!s.enabled || !SIMULATED.includes(s.kind)) continue;
      try {
        add(s.name, targetNeurons(s.targets, network));
      } catch (e) {
        problems.push(`${s.name}: ${e instanceof Error ? e.message : String(e)}`);
      }
    }
  }
  for (const name of selection.sets) {
    const set = network.sets.find((x) => x.name === name);
    if (set) add(name, [...set.members].sort((a, b) => a - b));
    else problems.push(`the network has no set '${name}'`);
  }
  if (selection.all) add("all neurons", network.neurons.map((n) => n.index));
  for (const m of selection.custom) {
    if (!m.pattern) continue;
    try {
      const neurons = matchNeurons(network.neurons, m).map((n) => n.index);
      if (neurons.length) add(customName(m), neurons);
      else problems.push(`no neurons have ${describeMatch(m)}`);
    } catch (e) {
      problems.push(e instanceof Error ? e.message : String(e));
    }
  }
  return { groups, problems };
}

/** What a test run gets as inputs: the file as Build gives it to a run (trials expanded with the seed), without the
 *  realtime sources (named in `skipped`: they aren't reproducible). */
export function testInputs(file: InputFile, seed: number, network: Network): { sources: InputSource[]; trials: Trial[]; skipped: string[] } {
  const expanded = expandTrials(file, seed);
  return {
    sources: engineSources(expanded.file, network),
    trials: expanded.trials,
    skipped: file.sources.filter((s) => s.enabled && isRealtime(s)).map((s) => s.name),
  };
}

/** The width of bin `i` in ms: `binMs`, but for a last bin cut short by where the run reached. */
export const binWidth = (r: Pick<TestRunResult, "binMs" | "simMs">, i: number) => Math.max(0, Math.min(r.binMs, r.simMs - i * r.binMs));

/** Spikes per neuron per second in each bin (0 for a group with no neurons). */
export function rates(r: TestRunResult, counts: number[], neurons: number): number[] {
  return counts.map((c, i) => {
    const w = binWidth(r, i);
    return neurons > 0 && w > 0 ? c / neurons / (w / 1000) : 0;
  });
}

export interface GroupSummary {
  name: string;
  neurons: number;
  spikes: number;
  /** Spikes per neuron per second over the time reached. */
  meanHz: number;
}

/** A row per group, the whole network last. */
export function summarize(r: TestRunResult, networkSize: number): GroupSummary[] {
  const row = (name: string, neurons: number, spikes: number): GroupSummary => ({
    name,
    neurons,
    spikes,
    meanHz: neurons > 0 && r.simMs > 0 ? spikes / neurons / (r.simMs / 1000) : 0,
  });
  return [...r.groups.map((g) => row(g.name, g.neurons, g.spikes)), row("whole network", networkSize, r.spikes)];
}

/** A condition's response: each group's rate (spikes per neuron per second) per bin from the trial's start, averaged
 *  over the condition's trials. */
export interface ConditionResponse {
  /** The condition's name ("" when the file has no conditions). */
  condition: string;
  /** The trials averaged. */
  trials: number;
  /** Bins from the trial's start (a trial's start is taken in the bin it falls in). */
  bins: number;
  groups: Array<{ name: string; rates: number[] }>;
}

/**
 * The trial-averaged response per condition (in order of first appearance): for each trial that the run completed,
 * each group's bins from the bin its start falls in, over the trial's length, averaged. A trial the run didn't reach
 * the end of (the budget stopped it) is left out.
 */
export function trialResponses(r: TestRunResult, trials: Trial[]): ConditionResponse[] {
  const done = trials.filter((t) => t.endMs <= r.simMs + 1e-9 && t.endMs > t.startMs);
  const names = [...new Set(done.map((t) => t.condition))];
  return names.map((condition) => {
    const mine = done.filter((t) => t.condition === condition);
    const first = (t: Trial) => Math.floor(t.startMs / r.binMs + 1e-9);
    const length = (t: Trial) => Math.max(1, Math.round((t.endMs - t.startMs) / r.binMs));
    const bins = Math.min(...mine.map(length));
    const groups = r.groups.map((g) => {
      const per = rates(r, g.counts, g.neurons);
      const sum = new Array<number>(bins).fill(0);
      for (const t of mine) for (let k = 0; k < bins; k++) sum[k]! += per[first(t) + k] ?? 0;
      return { name: g.name, rates: sum.map((v) => v / mine.length) };
    });
    return { condition, trials: mine.length, bins, groups };
  });
}

/** The conditions of the trials in order of first appearance (to give each a colour). */
export const conditionNames = (trials: Trial[]): string[] => [...new Set(trials.map((t) => t.condition))];

/** Names what a test run was made from, to tell whether the results are still those of the file as it is now. */
export const testKey = (inputs: string | null, file: InputFile | null, seed: number, session: number | null, description: string | null): string =>
  JSON.stringify([inputs, file, seed, session, description]);
