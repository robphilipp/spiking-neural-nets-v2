import type { NeuronState } from "@spikes/client";

/** How long ago a neuron last spiked, and when (simulated time): "120.5 ms ago (at 87.483 s)", or "—" if never. */
export function lastSpikeText(state: NeuronState): string {
  if (state.lastSpikeMs === null) return "—";
  const ago = Math.max(0, state.nowMs - state.lastSpikeMs);
  const since = ago < 1000 ? `${ago.toFixed(1)} ms` : `${(ago / 1000).toFixed(2)} s`;
  return `${since} ago (at ${(state.lastSpikeMs / 1000).toFixed(3)} s)`;
}
