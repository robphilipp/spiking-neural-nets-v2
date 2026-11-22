//! Records a run of a `.boo` network driven by Poisson input, then converts it to Parquet.
//!
//! ```text
//! cargo run --release -p spikes-record --example record -- <network.boo> <out-dir> [seconds] [shards]
//! ```
//!
//! Every neuron whose name starts with `input` gets 40 Hz Poisson input; every other neuron is observed, so its
//! membrane potential is recorded on each update.

use spikes_network::{BuildOptions, PoissonInput, Runtime, build};
use spikes_record::{RecordOptions, Recorder, RunInfo, to_parquet};
use tempo_runtime::{RuntimeConfig, SimDuration, SimTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, network, out, rest @ ..] = args.as_slice() else {
        eprintln!("usage: record <network.boo> <out-dir> [seconds] [shards]");
        std::process::exit(2);
    };
    let seconds: f64 = rest.first().map_or(Ok(2.0), |s| s.parse())?;
    let shards: usize = rest.get(1).map_or(Ok(1), |s| s.parse())?;
    let seed = 1;

    let text = std::fs::read_to_string(network)?;
    let description = spikes_dna::parse(&text)
        .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))?;
    let mut rt = Runtime::new(RuntimeConfig {
        seed,
        shards,
        min_remote_delay: Some(SimDuration::from_millis(1)),
        ..RuntimeConfig::default()
    });
    let net = build(
        &mut rt,
        &description,
        &BuildOptions {
            record_weights: true,
            ..BuildOptions::default()
        },
    );
    for n in net.matching(|name| !name.starts_with("input")) {
        rt.observe(n.id(), true);
    }
    let end = SimTime::from_start(SimDuration::from_secs_f64(seconds));
    rt.spawn(PoissonInput {
        rate_hz: 40.0,
        targets: net.matching(|name| name.starts_with("input")),
        mv: 3.0,
        delay: SimDuration::from_millis(1),
        start: SimTime::ZERO,
        until: end,
    });

    let recorder = Recorder::start(
        out,
        &rt,
        &net,
        &description,
        RunInfo {
            seed,
            description: &text,
        },
        RecordOptions::default(),
    )?;
    let started = std::time::Instant::now();
    rt.run_until(end);
    let summary = recorder.finish()?;
    println!("ran {seconds} s on {shards} shard(s) in {:?}", started.elapsed());
    println!("{summary:#?}");
    for file in to_parquet(out)? {
        println!("wrote {}", file.display());
    }
    Ok(())
}
