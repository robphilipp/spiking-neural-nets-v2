//! Phase 6: how big a network fits, and how fast it runs, on one machine.
//!
//! ```text
//! cargo run --release -p spikes-session --example scale -- [neurons] [synapses per neuron] [seconds] [shards]
//! ```
//!
//! A random network (neurons in a cube, each with `k` random inputs, 20% inhibitory), driven at ~5 Hz. Reports the
//! time and memory of each stage, then runs as fast as possible and reports throughput.

use std::time::Instant;

use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_dna::WeightSpec;
use spikes_session::{BuildSettings, Engine, InputSpec, RunSettings, Speed};
use tempo_runtime::SimDuration;

const TEMPLATE: &str = "(
NRN=[
    (nid=a, mst=10 mV, mpn=0 mV, mpd=20 ms, rfp=4 ms, spp=1 mV, csp=1 m/s, LOC=(cst=ct, px1=0, px2=0, px3=0)),
    (nid=b, mst=10 mV, mpn=0 mV, mpd=20 ms, rfp=4 ms, spp=1 mV, csp=1 m/s, LOC=(cst=ct, px1=1, px2=0, px3=0)),
],
CON=[(prn=a, psn=b, cnw=0.1)],
)";

/// Resident memory of this process, in GB (from `ps`).
fn rss_gb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().unwrap_or(0.0) / 1024.0 / 1024.0
}

fn stage(name: &str, t: Instant) {
    println!("{name:<28} {:>8.2} s   RSS {:>6.2} GB", t.elapsed().as_secs_f64(), rss_gb());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).map_or(200_000, |s| s.parse().expect("neurons"));
    let k: usize = args.get(2).map_or(100, |s| s.parse().expect("synapses per neuron"));
    let seconds: f64 = args.get(3).map_or(1.0, |s| s.parse().expect("seconds"));
    let shards: usize = args.get(4).map_or(8, |s| s.parse().expect("shards"));
    println!("{n} neurons × {k} synapses = {} synapses, {shards} shards", n * k);

    let t = Instant::now();
    let template = spikes_dna::parse(TEMPLATE).expect("template parses");
    let mut d = template.clone();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
    let side = 1000.0 * (n as f64 / 100_000.0).cbrt(); // constant density
    d.neurons = (0..n)
        .map(|i| {
            let mut neuron = template.neurons[0].clone();
            neuron.id = format!("n{i}");
            neuron.inhibitory = i % 5 == 0;
            neuron.location_um = std::array::from_fn(|_| rng.random_range(0.0..side));
            neuron
        })
        .collect();
    let w = 20.0 / k as f64; // keeps the total input per neuron about constant
    d.connections = (0..n * k)
        .map(|c| {
            let mut con = template.connections[0].clone();
            let pre = rng.random_range(0..n);
            con.pre = format!("n{pre}");
            con.post = format!("n{}", c / k);
            con.weight = WeightSpec::Fixed(w);
            con.equilibrium = WeightSpec::Fixed(w);
            con.sign = if pre % 5 == 0 {
                spikes_model::Sign::Inhibitory
            } else {
                spikes_model::Sign::Excitatory
            };
            con
        })
        .collect();
    stage("description generated", t);

    let engine = Engine::new();
    let session = engine.create_described("scale", "// generated", d);
    let t = Instant::now();
    let network = session
        .build(BuildSettings {
            shards,
            record_weights: false,
            ..BuildSettings::default()
        })
        .expect("build");
    stage("built", t);
    println!("  network summary lists {} synapses", network.synapses.len());
    let all: Vec<u32> = (0..n as u32).collect();
    for chunk in all.chunks(n.div_ceil(16)) {
        session
            .add_input(InputSpec::Poisson {
                targets: chunk.to_vec(),
                rate_hz: 5.0 * chunk.len() as f64,
                mv: 12.0,
                duration: None,
            })
            .expect("input");
    }
    let t = Instant::now();
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_secs_f64(seconds)),
            record: None,
        })
        .expect("start");
    assert!(session.wait_for_run(std::time::Duration::from_secs(3600)));
    let wall = t.elapsed().as_secs_f64();
    stage("ran", t);
    let spikes: u64 = (0..n as u32).step_by(n / 1000).map(|i| session.query_neuron(i).expect("query").spikes).sum::<u64>()
        * (n / 1000) as u64;
    println!(
        "  ~{:.1} Hz per neuron; {:.2} s wall per simulated second; ~{:.1} M synaptic events per wall second",
        spikes as f64 / n as f64 / seconds,
        wall / seconds,
        spikes as f64 * k as f64 / wall / 1e6
    );
}
