// A project: a folder with `project.toml`, the network description it names, and its input files.
//
//   name = "ring experiments"
//   network = "network.boo"
//   inputs = ["baseline.inputs.toml", "touch.inputs.toml"]   # input files (see inputFile.ts)
//   selected_inputs = "touch.inputs.toml"                    # the one runs use (optional: none)
//
//   [run]
//   seed = 1
//   shards = 2
//   speed = 1.0          # simulated s per wall s; "max" for as fast as possible
//   duration_s = 10      # omit to run until stopped
//
// A project from before input files, with a `[background]` table (rate_hz, mv, neurons, match), is read as one with
// a `background.inputs.toml` holding a Poisson source, selected (saving writes it out).

import { parse, stringify } from "smol-toml";
import type { InputFile } from "./inputFile";

export interface RunSettings {
  seed: number;
  shards: number;
  /** Simulated seconds per wall second; null: as fast as possible. */
  speed: number | null;
  /** Seconds; null: until stopped. */
  durationS: number | null;
}

export interface Project {
  name: string;
  /** The network description's file, relative to the project folder. */
  network: string;
  run: RunSettings;
  /** Input files, relative to the project folder. */
  inputs: string[];
  /** The input file runs use, if any (one of `inputs`). */
  selectedInputs: string | null;
}

export const DEFAULT_RUN: RunSettings = { seed: 1, shards: 2, speed: 1, durationS: null };

/** A new project's input file. */
export const DEFAULT_INPUTS = "inputs.inputs.toml";

export const newProject = (name: string): Project => ({
  name,
  network: "network.boo",
  run: { ...DEFAULT_RUN },
  inputs: [DEFAULT_INPUTS],
  selectedInputs: DEFAULT_INPUTS,
});

export class ProjectError extends Error {}

const number = (value: unknown, what: string, check: (n: number) => boolean): number => {
  if (typeof value !== "number" || !check(value)) throw new ProjectError(`project.toml: ${what} isn't valid (${String(value)})`);
  return value;
};

/** A file name inside the project folder. */
const inFolder = (file: string, what: string): string => {
  if (file.includes("..") || file.startsWith("/") || file.includes("\\")) throw new ProjectError(`project.toml: ${what} must be inside the project folder`);
  return file;
};

/** The input file made from a `[background]` table. */
export const MIGRATED_INPUTS = "background.inputs.toml";

/** What `project.toml` holds: the project, and (for a project from before input files) the input file made from
 *  its `[background]`. */
export interface ParsedProject {
  project: Project;
  migrated: InputFile | null;
}

/** Reads `project.toml`. */
export function parseProject(text: string): ParsedProject {
  let toml: Record<string, unknown>;
  try {
    toml = parse(text) as Record<string, unknown>;
  } catch (e) {
    throw new ProjectError(`project.toml: ${e instanceof Error ? e.message : String(e)}`);
  }
  const name = typeof toml.name === "string" ? toml.name : "untitled";
  const network = inFolder(typeof toml.network === "string" ? toml.network : "network.boo", "the network file");
  const run = (toml.run ?? {}) as Record<string, unknown>;
  const speed = run.speed === undefined ? DEFAULT_RUN.speed : run.speed === "max" ? null : number(run.speed, "run.speed", (n) => n > 0);
  let inputs: string[] = [];
  if (toml.inputs !== undefined) {
    if (!Array.isArray(toml.inputs) || !toml.inputs.every((f) => typeof f === "string")) {
      throw new ProjectError("project.toml: inputs must be a list of file names");
    }
    inputs = (toml.inputs as string[]).map((f) => inFolder(f, "an input file"));
  }
  let selectedInputs: string | null = typeof toml.selected_inputs === "string" ? toml.selected_inputs : null;
  if (selectedInputs !== null && !inputs.includes(selectedInputs)) {
    throw new ProjectError(`project.toml: selected_inputs '${selectedInputs}' isn't one of inputs`);
  }
  let migrated: InputFile | null = null;
  const background = toml.background as Record<string, unknown> | undefined;
  if (background) {
    const pattern = typeof background.neurons === "string" ? background.neurons : "";
    if (background.match !== undefined && background.match !== "prefix" && background.match !== "regex") {
      throw new ProjectError(`project.toml: background.match must be "prefix" or "regex" (got ${String(background.match)})`);
    }
    migrated = {
      description: "Background input (from project.toml's [background])",
      sources: [
        {
          name: "background",
          kind: "poisson",
          targets: background.match === "regex" ? { regex: pattern } : { prefix: pattern },
          start: 0,
          duration: null,
          enabled: true,
          hz: number(background.rate_hz, "background.rate_hz", (n) => n > 0),
          mv: number(background.mv, "background.mv", Number.isFinite),
        },
      ],
    };
    if (!inputs.includes(MIGRATED_INPUTS)) inputs = [...inputs, MIGRATED_INPUTS];
    selectedInputs = MIGRATED_INPUTS;
  }
  return {
    project: {
      name,
      network,
      run: {
        seed: run.seed === undefined ? DEFAULT_RUN.seed : number(run.seed, "run.seed", Number.isInteger),
        shards: run.shards === undefined ? DEFAULT_RUN.shards : number(run.shards, "run.shards", (n) => Number.isInteger(n) && n >= 1),
        speed,
        durationS: run.duration_s === undefined ? null : number(run.duration_s, "run.duration_s", (n) => n > 0),
      },
      inputs,
      selectedInputs,
    },
    migrated,
  };
}

/** Writes `project.toml`. */
export function serializeProject(p: Project): string {
  const run: Record<string, unknown> = { seed: p.run.seed, shards: p.run.shards, speed: p.run.speed ?? "max" };
  if (p.run.durationS !== null) run.duration_s = p.run.durationS;
  const toml: Record<string, unknown> = { name: p.name, network: p.network };
  if (p.inputs.length) toml.inputs = p.inputs;
  if (p.selectedInputs !== null) toml.selected_inputs = p.selectedInputs;
  toml.run = run;
  return stringify(toml) + "\n";
}
