//! `spikes`: check, run and record spiking networks from the command line.
//!
//! ```text
//! spikes check network.boo …
//! spikes run network.boo --for 10s --speed max --poisson input:40:3 --record runs/x --parquet
//! spikes parquet runs/x
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use spikes_session::{
    BuildSettings, Engine, InputSpec, RunSettings, Selection, Session, SessionError, SessionNotice, Speed, ViewSettings,
};
use tempo_runtime::SimDuration;

#[derive(Parser)]
#[command(name = "spikes", version, about = "Check, run and record spiking neural networks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parses network descriptions and reports errors or a summary.
    Check {
        /// `.boo` files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Builds and runs a network.
    Run(RunArgs),
    /// Converts a recorded run to Parquet (sorted by time, zstd-compressed).
    Parquet {
        /// The run directory.
        dir: PathBuf,
    },
    /// Converts descriptions in the older form (NRN, CON, …) to the current one, printing the result (or, with
    /// `--write`, replacing each file).
    Migrate {
        /// `.boo` files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Replace each file with its migrated form.
        #[arg(long)]
        write: bool,
    },
}

#[derive(clap::Args)]
struct RunArgs {
    /// The `.boo` file.
    file: PathBuf,
    /// How long to run, e.g. `500ms`, `10s`, `2m`. Without it a paced run goes on until interrupted, and a run as
    /// fast as possible ends when nothing is left to do.
    #[arg(long = "for", value_parser = parse_duration)]
    duration: Option<SimDuration>,
    /// `max` (as fast as possible) or simulated seconds per wall second (1 is real time).
    #[arg(long, default_value = "max", value_parser = parse_speed)]
    speed: Speed,
    /// Random seed.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Worker threads.
    #[arg(long, default_value_t = 1)]
    shards: usize,
    /// The shortest connection delay, e.g. `100us` (the default), `0.5ms`, or `0` for raw distance/speed delays.
    /// Very short delays make multi-shard runs synchronize often.
    #[arg(long, value_parser = parse_duration)]
    min_delay: Option<SimDuration>,
    /// Record into this directory.
    #[arg(long)]
    record: Option<PathBuf>,
    /// Also convert the recording to Parquet.
    #[arg(long, requires = "record")]
    parquet: bool,
    /// Poisson input `PREFIX:HZ:MV`: neurons whose names start with PREFIX (`*` for all) get HZ events per second in
    /// total, MV each. Repeatable.
    #[arg(long, value_name = "PREFIX:HZ:MV")]
    poisson: Vec<String>,
    /// Periodic input `PREFIX:MS:MV`: every matching neuron gets MV every MS milliseconds. Repeatable.
    #[arg(long, value_name = "PREFIX:MS:MV")]
    periodic: Vec<String>,
    /// One pulse `PREFIX:MV` at the start. Repeatable.
    #[arg(long, value_name = "PREFIX:MV")]
    stimulate: Vec<String>,
    /// Record the membrane potential of matching neurons on every update. Repeatable.
    #[arg(long, value_name = "PREFIX")]
    observe: Vec<String>,
    /// Don't print progress.
    #[arg(long, short)]
    quiet: bool,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Check { files } => check(&files),
        Command::Run(args) => run(&args),
        Command::Parquet { dir } => parquet(&dir),
        Command::Migrate { files, write } => migrate(&files, write),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn check(files: &[PathBuf]) -> Result<ExitCode> {
    let mut failed = false;
    for file in files {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        match spikes_dna::parse(&text) {
            Ok(d) => println!(
                "{}: ok: {} neurons{}, {} connections (expanded when built), {} sets, {} learning rules, {} \
                 modulators, {} nodes",
                file.display(),
                d.neurons.len(),
                match d.populations.len() {
                    0 => String::new(),
                    n => format!(" ({n} population{})", if n == 1 { "" } else { "s" }),
                },
                d.connections.len(),
                d.sets.len(),
                d.learning.len() - 1, // 'flat' is always there
                d.modulators.len(),
                d.nodes.len(),
            ),
            Err(errors) => {
                failed = true;
                for e in errors {
                    eprintln!("{}:{}:{}: {}", file.display(), e.span.line, e.span.column, e.message);
                }
            }
        }
    }
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn parse_duration(s: &str) -> std::result::Result<SimDuration, String> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (number, unit) = s.split_at(split);
    let value: f64 = number
        .parse()
        .map_err(|_| format!("'{s}' isn't a duration like 500ms or 10s"))?;
    let ms = match unit.trim() {
        "ns" => value / 1e6,
        "us" | "µs" => value / 1e3,
        "ms" => value,
        "s" | "" => value * 1e3,
        "m" | "min" => value * 60e3,
        "h" => value * 3600e3,
        other => return Err(format!("unknown time unit '{other}'")),
    };
    Ok(SimDuration::from_millis_f64(ms))
}

fn parse_speed(s: &str) -> std::result::Result<Speed, String> {
    if s == "max" {
        return Ok(Speed::AsFastAsPossible);
    }
    match s.parse::<f64>() {
        Ok(f) if f.is_finite() && f > 0.0 => Ok(Speed::Factor(f)),
        _ => Err(format!("speed must be 'max' or a positive number, not '{s}'")),
    }
}

/// Neurons whose names start with `prefix` (`*` for all).
fn matching(session: &Session, prefix: &str) -> Result<Vec<u32>> {
    let found: Vec<u32> = session
        .network()
        .neurons
        .iter()
        .filter(|n| prefix == "*" || n.name.starts_with(prefix))
        .map(|n| n.index)
        .collect();
    if found.is_empty() {
        return Err(format!("no neuron names start with '{prefix}'").into());
    }
    Ok(found)
}

/// Splits `PREFIX:A:B` from the right, so a prefix may itself contain ':'.
fn fields<const N: usize>(spec: &str, what: &str) -> Result<[String; N]> {
    let mut parts: Vec<String> = spec.rsplitn(N, ':').map(str::to_string).collect();
    parts.reverse();
    parts
        .try_into()
        .map_err(|_| format!("{what} '{spec}' needs {N} parts separated by ':'").into())
}

fn number<T: std::str::FromStr>(s: &str, spec: &str) -> Result<T> {
    s.parse()
        .map_err(|_| format!("'{s}' in '{spec}' isn't a number").into())
}

/// Spikes in a frame, whatever its level of detail.
fn spikes_in(bytes: &[u8]) -> u64 {
    match spikes_api::frame::decode(bytes) {
        Ok(f) => {
            f.spikes.len() as u64
                + f.spike_counts.iter().map(|c| u64::from(c.1)).sum::<u64>()
                + f.spike_bins.iter().map(|c| u64::from(c.1)).sum::<u64>()
        }
        Err(_) => 0,
    }
}

fn run(args: &RunArgs) -> Result<ExitCode> {
    let text = std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let engine = Engine::new();
    let name = args
        .file
        .file_stem()
        .map_or("network".into(), |s| s.to_string_lossy().to_string());
    let session = match engine.create(name, text) {
        Ok(session) => session,
        Err(SessionError::Parse(errors)) => {
            for e in errors {
                eprintln!(
                    "{}:{}:{}: {}",
                    args.file.display(),
                    e.span.line,
                    e.span.column,
                    e.message
                );
            }
            return Ok(ExitCode::FAILURE);
        }
        Err(e) => return Err(e.into()),
    };
    let started = Instant::now();
    let network = session.build(BuildSettings {
        seed: args.seed,
        shards: args.shards,
        min_delay: args.min_delay.unwrap_or(spikes_session::DEFAULT_MIN_DELAY),
        ..BuildSettings::default()
    })?;
    if !args.quiet {
        println!(
            "built {} neurons, {} synapses on {} shard(s) in {:.1?}",
            network.neurons.len(),
            network.synapses.len(),
            args.shards,
            started.elapsed()
        );
    }

    for spec in &args.poisson {
        let [prefix, hz, mv] = fields::<3>(spec, "--poisson")?;
        session.add_input(InputSpec::Poisson {
            targets: matching(&session, &prefix)?,
            rate_hz: number(&hz, spec)?,
            mv: number(&mv, spec)?,
            duration: args.duration,
        })?;
    }
    for spec in &args.periodic {
        let [prefix, ms, mv] = fields::<3>(spec, "--periodic")?;
        session.add_input(InputSpec::Periodic {
            targets: matching(&session, &prefix)?,
            period: SimDuration::from_millis_f64(number(&ms, spec)?),
            mv: number(&mv, spec)?,
            duration: args.duration,
        })?;
    }
    for spec in &args.stimulate {
        let [prefix, mv] = fields::<2>(spec, "--stimulate")?;
        session.stimulate(&matching(&session, &prefix)?, number(&mv, spec)?)?;
    }
    for prefix in &args.observe {
        session.observe(&matching(&session, prefix)?, true)?;
    }

    // Count spikes from live frames; a generous budget keeps them exact or per-neuron counts.
    let viewer = session.attach(ViewSettings {
        frame: Duration::from_millis(100),
        budget_bytes_per_s: 1 << 30,
        weights: Selection::Nothing,
        ..ViewSettings::default()
    });
    let notices = session.notices();
    let wall = Instant::now();
    session.start(RunSettings {
        speed: args.speed,
        duration: args.duration,
        record: args.record.clone(),
    })?;
    let mut spikes = 0u64;
    let mut last_print = Instant::now();
    loop {
        let ended = session.wait_for_run(Duration::from_millis(50));
        while let Some(frame) = viewer.try_recv() {
            spikes += spikes_in(&frame);
        }
        if ended {
            break;
        }
        if !args.quiet && last_print.elapsed() >= Duration::from_secs(1) {
            last_print = Instant::now();
            println!(
                "  t = {:>10.1} ms   {spikes} spikes",
                session.sim_time().since_start().as_millis_f64()
            );
        }
    }
    let elapsed = wall.elapsed();
    // The last frames arrive within one frame interval of the end.
    while let Some(frame) = viewer.recv_timeout(Duration::from_millis(250)) {
        spikes += spikes_in(&frame);
    }
    let sim = session.sim_time().since_start();
    println!(
        "ran {:.1} ms of simulated time in {elapsed:.2?} ({:.2}× real time): {spikes} spikes",
        sim.as_millis_f64(),
        sim.as_secs_f64() / elapsed.as_secs_f64()
    );
    let mut code = ExitCode::SUCCESS;
    for notice in notices.try_iter() {
        match notice {
            SessionNotice::RunEnded {
                recording: Some(summary),
                ..
            } => println!("recorded to {}: {:?}", summary.dir.display(), summary.rows),
            SessionNotice::RecordingFailed(e) | SessionNotice::Failed(e) => {
                eprintln!("error: {e}");
                code = ExitCode::FAILURE;
            }
            _ => {}
        }
    }
    if args.parquet
        && let Some(dir) = &args.record
    {
        parquet(dir)?;
    }
    Ok(code)
}

fn parquet(dir: &Path) -> Result<ExitCode> {
    for file in spikes_record::to_parquet(dir)? {
        println!("wrote {}", file.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn migrate(files: &[PathBuf], write: bool) -> Result<ExitCode> {
    let mut failed = false;
    for file in files {
        let text = std::fs::read_to_string(file)?;
        match spikes_dna::migrate(&text) {
            Ok(new) => {
                // The result must be a valid description.
                if let Err(errors) = spikes_dna::parse(&new) {
                    failed = true;
                    eprintln!("{}: migrated, but the result doesn't parse:", file.display());
                    for e in errors {
                        eprintln!("  {e}");
                    }
                    continue;
                }
                if write {
                    std::fs::write(file, new)?;
                    eprintln!("{}: migrated", file.display());
                } else {
                    print!("{new}");
                }
            }
            Err(errors) => {
                failed = true;
                for e in errors {
                    eprintln!("{}: {e}", file.display());
                }
            }
        }
    }
    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}
