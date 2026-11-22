//! Real-time pacing of the LIF network: runs it against the wall clock and reports how closely it kept time.
//!
//! ```sh
//! cargo run --release -p tempo-testkit --example realtime -- [neurons] [synapses] [sim_ms] [time_factor] [shards] [spin_us]
//! cargo run --release -p tempo-testkit --example realtime -- 10000 20 2000 1.0 8
//! ```

use std::time::Instant;

use tempo_runtime::{Pacing, Runtime, RuntimeConfig, SimDuration, SimTime};
use tempo_testkit::models::lif;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or_else(|| default.to_string());
    let neurons: usize = arg(1, "10000").parse().expect("neurons");
    let synapses: usize = arg(2, "20").parse().expect("synapses");
    let sim_ms: u64 = arg(3, "2000").parse().expect("sim_ms");
    let time_factor: f64 = arg(4, "1.0").parse().expect("time_factor");
    let shards: usize = arg(5, "8").parse().expect("shards");
    let spin_us: u64 = arg(6, "200").parse().expect("spin threshold µs");

    let network = lif::LifConfig {
        neurons,
        synapses_per_neuron: synapses,
        sources: neurons / 50,
        input_until: SimTime::MAX,
        ..lif::LifConfig::default()
    };
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 1,
        shards,
        pacing: Pacing::RealTime { time_factor },
        min_remote_delay: Some(network.input_delay),
        spin_threshold: std::time::Duration::from_micros(spin_us),
        ..RuntimeConfig::default()
    });
    lif::build(&mut rt, &network);

    let end = SimTime::from_start(SimDuration::from_millis(sim_ms));
    let started = Instant::now();
    let events = rt.run_until(end);
    let wall = started.elapsed();

    let pacing = rt.pacing_stats();
    let late = &pacing.lateness;
    println!("LIF network: {neurons} neurons × {synapses} synapses on {shards} shards, time factor {time_factor}");
    println!(
        "simulated:    {sim_ms} ms in {:.1} ms wall (target {:.1} ms)",
        wall.as_secs_f64() * 1e3,
        sim_ms as f64 * time_factor
    );
    println!(
        "events:       {events} ({:.1} M/s simulated)",
        events as f64 / (sim_ms as f64 / 1e3) / 1e6
    );
    println!("windows:      {}", pacing.windows);
    println!(
        "window start lateness vs clock:  p50 {:?}  p99 {:?}  p99.9 {:?}  max {:?}",
        late.quantile(0.5),
        late.quantile(0.99),
        late.quantile(0.999),
        late.max()
    );
}
