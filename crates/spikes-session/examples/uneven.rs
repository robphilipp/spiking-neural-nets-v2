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
    let neuron = "mst=10 mV, mpn=0 mV, mpd=20 ms, rfp=4 ms, spp=1 mV, csp=1 m/s";
    let text = format!(
        "(POP=[(nam=busy, cnt=40000, {neuron}, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=800, sx2=800, sx3=800)),
               (nam=calm, cnt=40000, {neuron}, REG=(cst=bx, px1=2000, px2=0, px3=0, sx1=800, sx2=800, sx3=800), sed=2)],
          RUL=[(prp=busy, psp=busy, cnt=200, cnw=0.1), (prp=calm, psp=calm, cnt=20, cnw=1)])"
    );
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
