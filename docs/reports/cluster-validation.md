# Lockstep and best effort: does late arrival act like benign noise?

Actors plan, phase 5 (2026-11-23). A cluster can run in **lockstep** (every window waits for every node: exact) or
**best effort** (each node follows its own copy of the clock; a spike that arrives after its time is delivered at
once). This report checks whether best effort's late spikes change what the network does, or only add noise of the
kind spiking networks shrug off.

## Method

One recurrent network with STDP learning, run six ways for 20 simulated seconds each, every run recorded, and
compared with run A:

- **A**: lockstep, at real time: the reference.
- **A′**: lockstep, as fast as possible. Pacing must never change results, so A′ must equal A exactly.
- **B, C**: best effort at real time and at twice real time: the runs under test.
- **D**: a single process with intentional timing noise: every connection delay jittered (normal, σ = the 99th
  percentile of B's lateness). This is noise known to be benign: it's a model parameter.
- **E**: a single process with another seed (other Poisson input): the network's own run-to-run variability.

Best effort passes if its differences from A are no larger than D's and E's.

Measures: per-neuron firing rates (correlation with A, mean absolute difference), the pooled inter-spike-interval
(ISI) distribution (two-sample Kolmogorov–Smirnov distance to A's), the mean coefficient of variation of ISIs, the
final weight distribution (KS distance), and the mean weight over time (largest difference, sampled every second).

Reproduce with:

```sh
cargo run --release -p spikes-session --example validate_cluster -- 1000 20 report.md 20 8
```

## Results

Network: 1000 neurons (20% inhibitory), 20000 synapses with STDP, Poisson drive 20 Hz × 8 mV per neuron; 20 s simulated per run; 3 nodes (1 shard each) over loopback TCP.

A' (lockstep, as fast as possible) is **identical** to A, spike for spike and weight for weight.

| Run | What | Wall time | Late events | Mean rate (Hz) | Rate correlation with A | Mean \|Δrate\| (Hz) | ISI KS distance to A | CV of ISI | Final weight KS distance to A | Max \|Δ mean weight\| over time |
|---|---|---|---|---|---|---|---|---|---|---|
| A | lockstep, real time (reference) | 20.0 s | — | 12.908 | 1.0000 | 0.0000 | 0.0000 | 0.805 | 0.0000 | 0.00000 |
| A' | lockstep, as fast as possible | 18.1 s | — | 12.908 | 1.0000 | 0.0000 | 0.0000 | 0.805 | 0.0000 | 0.00000 |
| B | best effort, real time | 20.1 s | 0.00% of 3723468 (p99 0.94 ms) | 12.908 | 1.0000 | 0.0000 | 0.0000 | 0.805 | 0.0001 | 0.00000 |
| C | best effort, 2× real time | 10.1 s | 0.04% of 3718442 (p99 5.02 ms) | 12.891 | 0.9951 | 0.1348 | 0.0010 | 0.804 | 0.0035 | 0.00092 |
| D | one process, intentional jitter | 1.7 s | — | 12.877 | 0.9897 | 0.1968 | 0.0021 | 0.804 | 0.0055 | 0.00160 |
| E | one process, another seed | 1.3 s | — | 12.758 | 0.8645 | 0.7293 | 0.0070 | 0.801 | 0.0107 | 0.00680 |

D's jitter: normal, σ = 0.94 ms on every connection delay.

## Findings

- **Lockstep is exact.** A′ equals A spike for spike and weight for weight, as the unit tests also show for every
  partitioning (identical traces on 1 and 3 nodes, in memory and over TCP).
- **At real time, best effort is as good as exact here.** Almost no spike arrived late; rates, ISIs and weights match
  A to four decimal places.
- **Twice real time gives late spikes, and they behave like noise.** 0.04% of 3.7 million cross-node spikes arrived
  late (99th percentile 5 ms). Every measure moved less than under D (jitter on *every* delay) and far less than
  under E (a different seed): rate correlation 0.995 against D's 0.990 and E's 0.86; ISI distance 0.001 against
  0.002 and 0.007; final-weight distance 0.0035 against 0.0055 and 0.011.
- Individual spike times do diverge once anything differs (spiking networks are chaotic), which is why D and E
  are the yardsticks rather than A: the statistics, the network's behaviour and its learning, don't change.
- Lockstep costs speed across nodes: A′ took 18 s for 20 simulated seconds (one network round trip per window,
  about 200,000 windows), against 1.3–1.7 s for one process. Best effort ran at the requested speed.

## Limits

- All three nodes ran on one machine over loopback TCP, so lateness came from scheduling, not a physical network.
  On a LAN, lateness per link will be larger. The panel's timeliness table (and this experiment, with node agents
  on other hosts) is how to check a real cluster; slowing the clock reduces lateness.
- One network and one seed per variant. The margins are wide, but more networks (for example, ones that depend on
  precise timing, such as synfire chains) would be a stronger test.
