//! Trains a winner-takes-all network the way the Inputs page would: one input neuron driven at a time, chosen
//! pseudo-randomly every block, by a rate-encoded designed signal injected before the run. Reports which output
//! follows which input, and the learned weights.
//!
//! ```text
//! cargo run --release -p spikes-session --example wta -- [file] [seconds] [block ms] [Hz] [mV]
//! ```

use spikes_session::{BuildSettings, Encoding, Engine, OnPause, RunSettings, SampleBatch, SensorSpec, Speed};
use tempo_runtime::SimDuration;

/// The Inputs page's formula: `floor(abs(sin(floor(t/block) * 12.9898) * 43758.5453)) % n == c`.
fn active(t_ms: f64, block_ms: f64, n: usize) -> usize {
    ((((t_ms / block_ms).floor() * 12.9898).sin() * 43758.5453).abs().floor() as usize) % n
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = args
        .get(1)
        .map_or("test-data/examples/wta-learning.boo", String::as_str);
    let seconds: f64 = args.get(2).map_or(120.0, |s| s.parse().expect("seconds"));
    let block_ms: f64 = args.get(3).map_or(300.0, |s| s.parse().expect("block ms"));
    let hz: f64 = args.get(4).map_or(100.0, |s| s.parse().expect("Hz"));
    let mv: f32 = args.get(5).map_or(10.0, |s| s.parse().expect("mV"));

    let engine = Engine::new();
    let session = engine
        .create("wta", std::fs::read_to_string(file).expect("file"))
        .expect("parses");
    let network = session.build(BuildSettings::default()).expect("builds");
    let index = |name: &str| network.by_name(name).expect(name);
    let count = |prefix: &str| {
        (1..)
            .take_while(|i| network.by_name(&format!("{prefix}{i}")).is_some())
            .count()
    };
    let inputs: Vec<u32> = (1..=count("in-")).map(|i| index(&format!("in-{i}"))).collect();
    let outputs: Vec<u32> = (1..=count("out-")).map(|i| index(&format!("out-{i}"))).collect();
    let n = inputs.len();

    // The whole signal, injected before running (as the page's Inject does).
    let mut spec = SensorSpec::new("designed signal", inputs.iter().map(|&i| vec![i]).collect());
    spec.encoding = Encoding::Rate { max_hz: hz, mv };
    spec.on_pause = OnPause::Buffer;
    spec.ingress_delay = SimDuration::from_millis(1);
    let sensor = session.connect_sensor(spec).expect("sensor");
    let steps = (seconds * 1000.0) as usize;
    let mut values = Vec::with_capacity(steps * n);
    for s in 0..steps {
        let on = active(s as f64, block_ms, n);
        values.extend((0..n).map(|c| if c == on { 1.0 } else { 0.0 }));
    }
    sensor
        .send_samples(&SampleBatch {
            t0_ns: 0,
            dt_ns: 1_000_000,
            values,
        })
        .expect("samples");

    // Run block by block, counting each output's spikes per active input over the last quarter.
    let blocks = (seconds * 1000.0 / block_ms) as usize;
    let mut counts = vec![vec![0u64; outputs.len()]; n]; // [input][output]
    let mut driven = vec![0u64; n]; // the active input's own spikes
    let spikes = |o: u32| session.query_neuron(o).expect("query").spikes;
    let mut before: Vec<u64> = outputs.iter().map(|&o| spikes(o)).collect();
    let mut before_in: Vec<u64> = inputs.iter().map(|&i| spikes(i)).collect();
    let mut window = vec![(0u64, 0u64); n];
    for b in 0..blocks {
        session
            .start(RunSettings {
                speed: Speed::AsFastAsPossible,
                duration: Some(SimDuration::from_millis_f64(block_ms)),
                record: None,
            })
            .expect("start");
        assert!(session.wait_for_run(std::time::Duration::from_secs(60)));
        let now: Vec<u64> = outputs.iter().map(|&o| spikes(o)).collect();
        let now_in: Vec<u64> = inputs.iter().map(|&i| spikes(i)).collect();
        if b >= blocks * 3 / 4 {
            let on = active(b as f64 * block_ms, block_ms, n);
            for o in 0..outputs.len() {
                counts[on][o] += now[o] - before[o];
            }
            driven[on] += now_in[on] - before_in[on];
        }
        // Over time: the share of each input's spikes its own output answered (out-k follows in-k), per 2 s.
        let on = active(b as f64 * block_ms, block_ms, n);
        if on < outputs.len() {
            window[on].0 += now_in[on] - before_in[on];
            window[on].1 += now[on] - before[on];
        }
        if ((b + 1) as f64 * block_ms) % 2000.0 < block_ms {
            let share: Vec<String> = window
                .iter()
                .map(|&(i, o)| {
                    if i == 0 {
                        "  -  ".into()
                    } else {
                        format!("{:4.0}%", 100.0 * o as f64 / i as f64)
                    }
                })
                .collect();
            println!(
                "  t={:5.1} s  followed: {}",
                (b + 1) as f64 * block_ms / 1e3,
                share.join(" ")
            );
            window = vec![(0, 0); n];
        }
        before = now;
        before_in = now_in;
    }

    println!("spikes while each input was active (last quarter):");
    println!(
        "          input   {}",
        (1..=outputs.len()).map(|o| format!("out-{o:<4}")).collect::<String>()
    );
    for (i, row) in counts.iter().enumerate() {
        println!(
            "  in-{}    {:<8}{}",
            i + 1,
            driven[i],
            row.iter().map(|c| format!("{c:<8}")).collect::<String>()
        );
    }
    println!("weights in → out:");
    for (o, &out) in outputs.iter().enumerate() {
        let state = session.query_neuron(out).expect("query");
        let w: Vec<String> = inputs
            .iter()
            .map(|&i| {
                state
                    .synapses
                    .iter()
                    .find(|s| s.0 == i)
                    .map_or("-".into(), |s| format!("{:.2}", s.1))
            })
            .collect();
        println!(
            "  out-{}  from in-1..: {}  (threshold shift {:.2} mV)",
            o + 1,
            w.join("  "),
            state.intrinsic_mv
        );
    }
}
