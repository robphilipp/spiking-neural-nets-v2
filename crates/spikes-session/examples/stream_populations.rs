//! How the live view keeps up with a learning network: the `populations` example (4,000 neurons, 220k synapses,
//! STDP on the excitatory ones) at real time, viewed as the Run page views it.
//!
//! ```text
//! cargo run --release -p spikes-session --example stream_populations -- [seconds] [shards] [hz] [mv] [weights]
//! ```
//!
//! `weights`: `view` (default; the weights panel's viewer, every synapse), `noview`, or `norecord` (weight changes
//! aren't emitted at all). Reports the events the live view received and dropped, and what routing them cost.

use std::time::{Duration, Instant};

use spikes_api::frame::decode;
use spikes_session::{BuildSettings, Engine, InputSpec, RunSettings, Selection, Speed, ViewSettings, Viewer};

fn take(viewer: &Viewer, dropped: &mut u32, frames: &mut u64) {
    while let Some(bytes) = viewer.try_recv() {
        let f = decode(&bytes).expect("valid frame");
        *dropped = f.header.dropped_events;
        *frames += 1;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seconds: f64 = args.get(1).map_or(10.0, |s| s.parse().expect("seconds"));
    let shards: usize = args.get(2).map_or(2, |s| s.parse().expect("shards"));
    let hz: f64 = args.get(3).map_or(5.0, |s| s.parse().expect("hz"));
    let mv: f64 = args.get(4).map_or(12.0, |s| s.parse().expect("mv"));
    let weights = args.get(5).map_or("view", String::as_str);

    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-data/examples/populations.boo"
    ))
    .expect("populations.boo");
    let engine = Engine::new();
    let session = engine.create("populations", &text).expect("parses");
    let network = session
        .build(BuildSettings {
            shards,
            record_weights: weights != "norecord",
            ..BuildSettings::default()
        })
        .expect("build");
    let exc: Vec<u32> = network
        .neurons
        .iter()
        .filter(|n| n.name.starts_with("exc-"))
        .map(|n| n.index)
        .collect();
    session
        .add_input(InputSpec::Poisson {
            targets: exc.clone(),
            rate_hz: hz * exc.len() as f64,
            mv: mv as f32,
            duration: None,
        })
        .expect("input");

    // The Run page's viewers: the whole network (rate, 3D), a raster, and the weights panel.
    let whole = session.attach(ViewSettings {
        budget_bytes_per_s: 1 << 20,
        weights: Selection::Nothing,
        ..ViewSettings::default()
    });
    let raster = session.attach(ViewSettings {
        spikes: Selection::Neurons((0..300).collect()),
        weights: Selection::Nothing,
        ..ViewSettings::default()
    });
    let weight_view = (weights == "view").then(|| {
        session.attach(ViewSettings {
            spikes: Selection::Nothing,
            weights: Selection::All,
            ..ViewSettings::default()
        })
    });

    session
        .start(RunSettings {
            speed: Speed::Factor(1.0),
            duration: Some(tempo_runtime::SimDuration::from_secs_f64(seconds)),
            record: None,
        })
        .expect("start");
    let wall = Instant::now();
    let (mut dropped, mut frames) = (0u32, 0u64);
    while !session.wait_for_run(Duration::from_millis(2)) {
        take(&whole, &mut dropped, &mut frames);
        take(&raster, &mut dropped, &mut frames);
        if let Some(w) = &weight_view {
            take(w, &mut dropped, &mut frames);
        }
    }
    let elapsed = wall.elapsed().as_secs_f64();
    std::thread::sleep(Duration::from_millis(200));
    take(&whole, &mut dropped, &mut frames);

    let stats = session.frame_stats();
    println!(
        "{weights}: {:.2} s simulated in {elapsed:.2} s on {shards} shards; {} neurons, {} synapses",
        session.sim_time().since_start().as_secs_f64(),
        network.neurons.len(),
        network.synapse_count
    );
    println!(
        "live view: {} events received ({:.0}/s), {} dropped ({:.0}/s); routing {:.0} ns/event ({:.0}% of one core); {frames} frames",
        stats.events,
        stats.events as f64 / elapsed,
        dropped,
        f64::from(dropped) / elapsed,
        stats.routing.as_nanos() as f64 / stats.events.max(1) as f64,
        stats.routing.as_secs_f64() / elapsed * 100.0
    );
    println!(
        "frames: {} built, encoding {:.0}% of one core (mean {:.2?}, slowest {:.2?}); aggregator busy {:.0}% of one core",
        stats.frames,
        stats.encoding.as_secs_f64() / elapsed * 100.0,
        stats.encoding / u32::try_from(stats.frames.max(1)).unwrap_or(u32::MAX),
        stats.slowest_frame,
        (stats.routing + stats.encoding).as_secs_f64() / elapsed * 100.0
    );
}
