//! Parallel scaling of the LIF network: runs the same network on several shard counts, reports throughput and
//! speedup, and checks that every run produces exactly the same spikes.
//!
//! ```sh
//! cargo run --release -p tempo-testkit --example lif_scaling -- [neurons] [synapses] [sim_ms] [shards,...]
//! cargo run --release -p tempo-testkit --example lif_scaling -- 100000 100 50 1,2,4,8,10
//! ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use tempo_runtime::{Runtime, RuntimeConfig, SimDuration, SimTime};
use tempo_testkit::models::lif;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize, default: &str| args.get(i).cloned().unwrap_or_else(|| default.to_string());
    let neurons: usize = arg(1, "100000").parse().expect("neurons");
    let synapses: usize = arg(2, "100").parse().expect("synapses");
    let sim_ms: u64 = arg(3, "50").parse().expect("sim_ms");
    let shard_counts: Vec<usize> = arg(4, "1,2,4,8,10")
        .split(',')
        .map(|s| s.parse().expect("shard count"))
        .collect();

    let network = lif::LifConfig {
        neurons,
        synapses_per_neuron: synapses,
        // Keep the input per neuron the same as the 10k-neuron default (20 sources at 2 kHz per 1,000 neurons).
        sources: neurons / 50,
        input_until: SimTime::MAX,
        ..lif::LifConfig::default()
    };
    let end = SimTime::from_start(SimDuration::from_millis(sim_ms));
    println!("LIF network: {neurons} neurons × {synapses} synapses, {sim_ms} ms simulated");
    println!(
        "{:>6} {:>9} {:>9} {:>12} {:>9} {:>8} {:>10} {:>16}",
        "shards", "build s", "run s", "events", "Mev/s", "speedup", "lookahead", "spikes hash"
    );

    let mut baseline = None;
    let mut reference_hash = None;
    for &shards in &shard_counts {
        let built = Instant::now();
        let mut rt = Runtime::new(RuntimeConfig {
            seed: 1,
            shards,
            min_remote_delay: Some(network.input_delay),
            ..RuntimeConfig::default()
        });
        rt.buffer_events();
        lif::build(&mut rt, &network);
        let build_s = built.elapsed().as_secs_f64();

        let started = Instant::now();
        let events = rt.run_until(end);
        let run_s = started.elapsed().as_secs_f64();

        let mut hasher = DefaultHasher::new();
        for e in rt.take_events() {
            (e.at, e.src).hash(&mut hasher);
        }
        let hash = hasher.finish();
        let identical = *reference_hash.get_or_insert(hash) == hash;
        let base = *baseline.get_or_insert(run_s);
        println!(
            "{:>6} {:>9.2} {:>9.2} {:>12} {:>9.1} {:>7.2}× {:>10} {:>16x}{}",
            shards,
            build_s,
            run_s,
            events,
            events as f64 / run_s / 1e6,
            base / run_s,
            if rt.lookahead() == SimDuration::MAX {
                "—".to_string()
            } else {
                rt.lookahead().to_string()
            },
            hash,
            if identical { "" } else { "  <-- DIFFERS" }
        );
        assert_eq!(rt.stats().lookahead_violations, 0);
    }
}
