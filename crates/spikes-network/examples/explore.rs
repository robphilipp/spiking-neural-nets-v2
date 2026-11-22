//! Runs a v1 network and prints its spikes: `cargo run -p spikes-network --example explore -- ring.boo hidden-10 200`.
use spikes_network::{BuildOptions, Event, NeuronMsg, Runtime, build};
use tempo_runtime::{RuntimeConfig, SimDuration, SimTime};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = &args[1];
    let prefix = &args[2];
    let ms: u64 = args[3].parse().unwrap();
    let path = format!("{}/../../test-data/v1/{file}", env!("CARGO_MANIFEST_DIR"));
    let description = spikes_dna::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 1,
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    let net = build(&mut rt, &description, &BuildOptions::default());
    for n in net.matching(|name| name.starts_with(prefix.as_str())) {
        rt.inject(n.id(), SimTime::ZERO, NeuronMsg::External { mv: 10.0 }.into());
    }
    rt.run_until(SimTime::from_start(SimDuration::from_millis(ms)));
    for e in rt.take_events() {
        match e.event {
            Event::Spike { at } => println!(
                "{:>10.3} ms spike  {}",
                at.since_start().as_millis_f64(),
                net.name(e.src).unwrap()
            ),
            Event::Weight { pre, before, after, .. } => println!(
                "{:>10.3} ms weight {} <- {}: {before:.4} -> {after:.4}",
                e.at.since_start().as_millis_f64(),
                net.name(e.src).unwrap(),
                net.name(pre).unwrap()
            ),
            other => println!(
                "{:>10.3} ms {other:?} {}",
                e.at.since_start().as_millis_f64(),
                net.name(e.src).unwrap()
            ),
        }
    }
}
