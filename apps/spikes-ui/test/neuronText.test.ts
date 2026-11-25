import type { NeuronState } from "@spikes/client";
import { describe, expect, it } from "vitest";
import { lastSpikeText } from "../src/neuronText";

const state = (lastSpikeMs: number | null, nowMs: number): NeuronState => ({
  membraneMv: 0,
  intrinsicMv: 0,
  rateHz: 0,
  lastEventMs: nowMs,
  lastSpikeMs,
  nowMs,
  tonic: false,
  spikes: 0,
  synapses: [],
});

describe("last spike", () => {
  it("is how long ago, and when", () => {
    expect(lastSpikeText(state(87_483, 87_603.5))).toBe("120.5 ms ago (at 87.483 s)");
    expect(lastSpikeText(state(1_000, 4_500))).toBe("3.50 s ago (at 1.000 s)");
  });

  it("is a dash for a neuron that hasn't spiked", () => {
    expect(lastSpikeText(state(null, 500))).toBe("—");
  });
});
