//! Placement on a network whose two halves do very different amounts of work: one region's neurons have 200 inputs
//! each, the other's 20. Balanced placement gives shards equal work, not equal neuron counts.
//!
//! ```text
//! cargo run --release -p spikes-session --example uneven -- [shards]
//! ```
use spikes_session::{BuildSettings, Engine, InputSpec, RunSettings, Speed};
use tempo_runtime::SimDuration;

fn main() {
    let shards: usize = std::env::args().nth(1).map_or(8, |s| s.parse().expect("shards"));
    let text = "(
neuron_types=[(name=cell, threshold=10 mV, membrane_noise=0 mV, decay_time=20 ms, refractory_period=4 ms, conduction_speed=1 m/s)],
populations=[
    (name=busy, type=cell, count=40000, random=(box=(corner=[0, 0, 0], size=[800, 800, 800]))),
    (name=calm, type=cell, count=40000, random=(box=(corner=[2000, 0, 0], size=[800, 800, 800])), seed=2),
],
connections=[
    (from=busy, to=busy, pattern=fan_in, count=200, weight=0.1),
    (from=calm, to=calm, pattern=fan_in, count=20, weight=1),
],
)";
    let engine = Engine::new();
    let session = engine.create("uneven", text).expect("parses");
    session
        .build(BuildSettings {
            shards,
            record_weights: false,
            ..BuildSettings::default()
        })
        .expect("builds");
    for chunk in (0..80_000u32).collect::<Vec<_>>().chunks(5000) {
        session
            .add_input(InputSpec::Poisson {
                targets: chunk.to_vec(),
                rate_hz: 5.0 * 5000.0,
                mv: 12.0,
                duration: None,
            })
            .expect("input");
    }
    let t = std::time::Instant::now();
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_secs(1)),
            record: None,
        })
        .expect("starts");
    assert!(session.wait_for_run(std::time::Duration::from_secs(600)));
    let wall = t.elapsed().as_secs_f64();
    let (_, lookahead, wait) = session.runtime_stats().expect("ready");
    println!(
        "{shards} shards: {wall:.2} s wall per simulated second; lookahead {:.3} ms; shards waited {:.0}% of their time",
        lookahead.as_millis_f64(),
        100.0 * wait.as_secs_f64() / (wall * shards as f64)
    );
}
