// What the Inputs page's test panel keeps while you move between pages: its settings and the last run's result.

import type { TestRunResult } from "@spikes/client";
import { create } from "zustand";
import type { Trial } from "./inputFile";
import { DEFAULT_BIN_MS, emptySelection, type GroupSelection } from "./testRun";

/** What a finished test run shows: the engine's counts, and what it was made from. */
export interface Outcome {
  /** See `testKey`: what the run was made from. */
  key: string;
  result: TestRunResult;
  /** The trials the run was given (as the run's seed drew them). */
  trials: Trial[];
  /** The network's neuron count. */
  size: number;
}

interface TestState {
  selection: GroupSelection;
  /** Seconds; null: the default (the end of the trials, else 5 s). */
  durationS: number | null;
  binMs: number;
  outcome: Outcome | null;
}

export const useTest = create<TestState>()(() => ({
  selection: emptySelection(),
  durationS: null,
  binMs: DEFAULT_BIN_MS,
  outcome: null,
}));
