//! A1 acceptance: a paced 100k-neuron network streams live frames at 60 fps, within budget, cheaply.
//!
//! ```text
//! cargo run --release -p spikes-session --example stream_100k -- [neurons] [seconds] [shards]
//! ```
//!
//! 100k neurons in a 1 mm cube, 10 random inputs each (1M synapses), every neuron driven at ~5 Hz by Poisson input
//! strong enough to fire it: about 500k spikes per second. Two viewers: the whole network with the default 4 MB/s
//! budget, and a 300-neuron selection (a raster's worth).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_api::frame::{Lod, decode};
use spikes_dna::WeightSpec;
use spikes_session::{BuildSettings, Engine, InputSpec, RunSettings, Selection, Speed, ViewSettings, Viewer};
use tempo_runtime::SimDuration;

const TEMPLATE: &str = "(
NRN=[
    (nid=a, mst=10 mV, mpn=0 mV, mpd=20 ms, rfp=5 ms, spp=1 mV, csp=1 m/s, LOC=(cst=ct, px1=0, px2=0, px3=0)),
    (nid=b, mst=10 mV, mpn=0 mV, mpd=20 ms, rfp=5 ms, spp=1 mV, csp=1 m/s, LOC=(cst=ct, px1=1, px2=0, px3=0)),
],
CON=[(prn=a, psn=b, cnw=0.5)],
)";

#[derive(Default)]
struct Seen {
    frames: u64,
    spikes: u64,
    lods: BTreeMap<String, u64>,
    gaps: Vec<Duration>,
    last: Option<Instant>,
    bytes: u64,
    max_lag_ms: f32,
    dropped_frames: u32,
    dropped_events: u32,
}

impl Seen {
    fn take(&mut self, viewer: &Viewer) {
        while let Some(bytes) = viewer.try_recv() {
            let now = Instant::now();
            if let Some(last) = self.last {
                self.gaps.push(now - last);
            }
            self.last = Some(now);
            self.bytes += bytes.len() as u64;
            let f = decode(&bytes).expect("valid frame");
            self.frames += 1;
            self.spikes += f.spikes.len() as u64
                + f.spike_counts.iter().map(|c| u64::from(c.1)).sum::<u64>()
                + f.spike_bins.iter().map(|c| u64::from(c.1)).sum::<u64>();
            let lod = match f.header.lod {
                Lod::Exact => "exact",
                Lod::Counts => "counts",
                Lod::Binned => "cells",
            };
            *self.lods.entry(lod.to_string()).or_default() += 1;
            self.max_lag_ms = self.max_lag_ms.max(f.header.lag_ms);
            self.dropped_frames = f.header.dropped_frames;
            self.dropped_events = f.header.dropped_events;
        }
    }

    fn report(&mut self, name: &str, seconds: f64) {
        self.gaps.sort();
        let pct = |p: f64| {
            self.gaps
                .get(((self.gaps.len() as f64 - 1.0) * p) as usize)
                .copied()
                .unwrap_or_default()
        };
        println!(
            "{name}: {} frames ({:.1}/s), gaps p50 {:.1?} p99 {:.1?} max {:.1?}; {:.2} MB/s; {} spikes; lod {:?}; \
             max lag {:.1} ms; dropped {} frames, {} events",
            self.frames,
            self.frames as f64 / seconds,
            pct(0.5),
            pct(0.99),
            pct(1.0),
            self.bytes as f64 / seconds / 1e6,
            self.spikes,
            self.lods,
            self.max_lag_ms,
            self.dropped_frames,
            self.dropped_events
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).map_or(100_000, |s| s.parse().expect("neurons"));
    let seconds: f64 = args.get(2).map_or(5.0, |s| s.parse().expect("seconds"));
    let shards: usize = args.get(3).map_or(4, |s| s.parse().expect("shards"));
    let speed = match args.get(4).map(String::as_str) {
        Some("max") => Speed::AsFastAsPossible,
        Some(f) => Speed::Factor(f.parse().expect("speed")),
        None => Speed::Factor(1.0),
    };
    let view = args.get(5).map(String::as_str) != Some("noview");
    let min_delay_us: u64 = args.get(6).map_or(100, |s| s.parse().expect("min delay µs"));

    // Generate the description from a parsed template, so every default comes from the parser.
    let template = spikes_dna::parse(TEMPLATE).expect("template parses");
    let mut description = template.clone();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(42);
    description.neurons = (0..n)
        .map(|i| {
            let mut neuron = template.neurons[0].clone();
            neuron.id = format!("n{i}");
            neuron.location_um = [
                rng.random_range(0.0..1000.0),
                rng.random_range(0.0..1000.0),
                rng.random_range(0.0..1000.0),
            ];
            neuron
        })
        .collect();
    description.connections = (0..n * 10)
        .map(|k| {
            let mut c = template.connections[0].clone();
            c.pre = format!("n{}", rng.random_range(0..n));
            c.post = format!("n{}", k / 10);
            c.weight = WeightSpec::Fixed(0.5);
            c.equilibrium = WeightSpec::Fixed(0.5);
            c
        })
        .collect();

    let engine = Engine::new();
    let session = engine.create_described("stream", format!("// generated: {n} neurons"), description);
    let t = Instant::now();
    let network = session
        .build(BuildSettings {
            shards,
            min_delay: SimDuration::from_micros(min_delay_us),
            record_weights: false,
            ..BuildSettings::default()
        })
        .expect("build");
    println!(
        "built {} neurons, {} synapses on {shards} shards in {:.2?}",
        network.neurons.len(),
        network.synapses.len(),
        t.elapsed()
    );
    // ~5 Hz per neuron, 12 mV (over the 10 mV threshold) per event, split over 8 input actors.
    let all: Vec<u32> = (0..n as u32).collect();
    for chunk in all.chunks(n.div_ceil(8)) {
        session
            .add_input(InputSpec::Poisson {
                targets: chunk.to_vec(),
                rate_hz: 5.0 * chunk.len() as f64,
                mv: 12.0,
                duration: None,
            })
            .expect("input");
    }

    let nothing = ViewSettings {
        spikes: Selection::Nothing,
        weights: Selection::Nothing,
        ..ViewSettings::default()
    };
    let whole = session.attach(if view {
        ViewSettings {
            weights: Selection::Nothing,
            ..ViewSettings::default()
        }
    } else {
        nothing.clone()
    });
    let raster = session.attach(if view {
        ViewSettings {
            spikes: Selection::Neurons((0..300).collect()),
            weights: Selection::Nothing,
            ..ViewSettings::default()
        }
    } else {
        nothing
    });
    let (mut seen_whole, mut seen_raster) = (Seen::default(), Seen::default());
    session
        .start(RunSettings {
            speed,
            duration: Some(SimDuration::from_secs_f64(seconds)),
            record: None,
        })
        .expect("start");
    let wall = Instant::now();
    while !session.wait_for_run(Duration::from_millis(2)) {
        seen_whole.take(&whole);
        seen_raster.take(&raster);
    }
    let elapsed = wall.elapsed().as_secs_f64();
    std::thread::sleep(Duration::from_millis(100));
    seen_whole.take(&whole);
    seen_raster.take(&raster);

    println!(
        "ran {:.2} s of simulated time in {elapsed:.2} s",
        session.sim_time().since_start().as_secs_f64()
    );
    seen_whole.report("whole network", elapsed);
    seen_raster.report("300-neuron raster", elapsed);
    let stats = session.frame_stats();
    println!(
        "aggregator: {} events, routing {:.1} ns/event ({:.1?} total); {} frames, encoding mean {:.2?}, slowest {:.2?}",
        stats.events,
        stats.routing.as_nanos() as f64 / stats.events.max(1) as f64,
        stats.routing,
        stats.frames,
        stats.encoding / u32::try_from(stats.frames.max(1)).unwrap_or(u32::MAX),
        stats.slowest_frame
    );
}
