// A project: a folder with `project.toml` and the network description it names.
//
//   name = "ring experiments"
//   network = "network.boo"
//
//   [run]
//   seed = 1
//   shards = 2
//   speed = 1.0          # simulated s per wall s; "max" for as fast as possible
//   duration_s = 10      # omit to run until stopped
//
//   [background]         # optional: Poisson input
//   rate_hz = 5          # per neuron
//   mv = 12
//   neurons = "input-"   # only these neurons: names starting with this (default: all)
//   match = "regex"      # optional: `neurons` is a regular expression instead ("prefix", the default)

import { parse, stringify } from "smol-toml";
import type { NeuronMatch } from "./neuronMatch";

export interface RunSettings {
  seed: number;
  shards: number;
  /** Simulated seconds per wall second; null: as fast as possible. */
  speed: number | null;
  /** Seconds; null: until stopped. */
  durationS: number | null;
}

export interface Background {
  rateHz: number;
  mv: number;
  /** Which neurons (an empty pattern: all). */
  neurons: NeuronMatch;
}

export interface Project {
  name: string;
  /** The network description's file, relative to the project folder. */
  network: string;
  run: RunSettings;
  background: Background | null;
}

export const DEFAULT_RUN: RunSettings = { seed: 1, shards: 2, speed: 1, durationS: null };

export const newProject = (name: string): Project => ({ name, network: "network.boo", run: { ...DEFAULT_RUN }, background: null });

export class ProjectError extends Error {}

const number = (value: unknown, what: string, check: (n: number) => boolean): number => {
  if (typeof value !== "number" || !check(value)) throw new ProjectError(`project.toml: ${what} isn't valid (${String(value)})`);
  return value;
};

const matchMode = (value: unknown): "regex" => {
  if (value === "regex") return "regex";
  throw new ProjectError(`project.toml: background.match must be "prefix" or "regex" (got ${String(value)})`);
};

/** Reads `project.toml`. */
export function parseProject(text: string): Project {
  let toml: Record<string, unknown>;
  try {
    toml = parse(text) as Record<string, unknown>;
  } catch (e) {
    throw new ProjectError(`project.toml: ${e instanceof Error ? e.message : String(e)}`);
  }
  const name = typeof toml.name === "string" ? toml.name : "untitled";
  const network = typeof toml.network === "string" ? toml.network : "network.boo";
  if (network.includes("..") || network.startsWith("/")) throw new ProjectError("project.toml: the network file must be inside the project folder");
  const run = (toml.run ?? {}) as Record<string, unknown>;
  const speed = run.speed === undefined ? DEFAULT_RUN.speed : run.speed === "max" ? null : number(run.speed, "run.speed", (n) => n > 0);
  const background = toml.background as Record<string, unknown> | undefined;
  return {
    name,
    network,
    run: {
      seed: run.seed === undefined ? DEFAULT_RUN.seed : number(run.seed, "run.seed", Number.isInteger),
      shards: run.shards === undefined ? DEFAULT_RUN.shards : number(run.shards, "run.shards", (n) => Number.isInteger(n) && n >= 1),
      speed,
      durationS: run.duration_s === undefined ? null : number(run.duration_s, "run.duration_s", (n) => n > 0),
    },
    background: background
      ? {
          rateHz: number(background.rate_hz, "background.rate_hz", (n) => n > 0),
          mv: number(background.mv, "background.mv", Number.isFinite),
          neurons: {
            mode: background.match === undefined || background.match === "prefix" ? "prefix" : matchMode(background.match),
            pattern: typeof background.neurons === "string" ? background.neurons : "",
          },
        }
      : null,
  };
}

/** Writes `project.toml`. */
export function serializeProject(p: Project): string {
  const run: Record<string, unknown> = { seed: p.run.seed, shards: p.run.shards, speed: p.run.speed ?? "max" };
  if (p.run.durationS !== null) run.duration_s = p.run.durationS;
  const toml: Record<string, unknown> = { name: p.name, network: p.network, run };
  if (p.background) {
    const b: Record<string, unknown> = { rate_hz: p.background.rateHz, mv: p.background.mv };
    if (p.background.neurons.pattern) b.neurons = p.background.neurons.pattern;
    if (p.background.neurons.mode === "regex") b.match = "regex";
    toml.background = b;
  }
  return stringify(toml) + "\n";
}
