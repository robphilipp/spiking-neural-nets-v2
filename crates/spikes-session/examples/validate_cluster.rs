//! Actors phase 5: does best-effort lateness act like benign noise?
//!
//! ```text
//! cargo run --release -p spikes-session --example validate_cluster -- [neurons] [seconds] [report.md] [drive Hz] [drive mV]
//! ```
//!
//! Runs one recurrent network with STDP learning on a 3-node cluster (two node agents in this process, over loopback
//! TCP) in several ways, records each run, and compares spike and weight statistics with the lockstep (exact) run:
//!
//! - A  lockstep at real time: the reference;
//! - A' lockstep as fast as possible: must be identical to A (pacing never changes results);
//! - B  best effort at real time, C best effort at twice real time: the runs under test;
//! - D  one process with intentional delay jitter about the size of B's lateness: known, benign timing noise;
//! - E  one process with another seed: the network's own run-to-run variability.
//!
//! Best effort passes if its differences from A are no larger than D's and E's.

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use arrow::array::{Array, Float32Array, UInt32Array, UInt64Array};
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_dna::{NetworkDescription, WeightSpec};
use spikes_session::{BuildSettings, Engine, InputSpec, RunSettings, SessionNotice, Speed};
use tempo_runtime::{Jitter, SimDuration};

const TEMPLATE: &str = "(
neuron_types=[
    (name=neuron, threshold=10 mV, membrane_noise=0 mV, decay_time=20 ms, refractory_period=4 ms, spike_potential=1 mV, conduction_speed=1 m/s),
],
synapse_types=[
    (name=stdp, decay=(time=1500 ms), limits=(min=0, max=3), release=(), learning=stdp),
],
neurons=[
    (name=a, type=neuron, position=[0, 0, 0]),
    (name=b, type=neuron, position=[1, 0, 0]),
],
connections=[
    (from=a, to=b, type=stdp, weight=1),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.06, depression_time=30 ms, potentiation=0.05, potentiation_time=20 ms),
],
)";

/// A random recurrent network: `n` neurons in a 1 mm cube, 20 excitatory (80%) or inhibitory inputs each, learning.
fn network(n: usize) -> NetworkDescription {
    let template = spikes_dna::parse(TEMPLATE).expect("template parses");
    let mut d = template.clone();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(7);
    d.neurons = (0..n)
        .map(|i| {
            let mut neuron = template.neurons.get(0).unwrap().to_description();
            neuron.id = format!("n{i}");
            neuron.inhibitory = i % 5 == 0;
            neuron.location_um = std::array::from_fn(|_| rng.random_range(0.0..1000.0));
            neuron
        })
        .collect();
    d.connections = (0..n * 20)
        .map(|k| {
            let mut c = template.connections[0].clone();
            let pre = rng.random_range(0..n);
            c.from = vec![pre as u32];
            c.to = vec![(k / 20) as u32];
            let w = rng.random_range(0.5..1.5);
            c.synapse.weight = WeightSpec::Fixed(w);
            c.synapse.equilibrium = None;
            c.synapse.sign = Some(if pre % 5 == 0 {
                spikes_model::Sign::Inhibitory
            } else {
                spikes_model::Sign::Excitatory
            });
            c
        })
        .collect();
    d
}

struct Variant {
    name: &'static str,
    what: &'static str,
    nodes: bool,
    lockstep: bool,
    speed: Speed,
    seed: u64,
    jitter: Jitter,
}

/// One recorded run, reduced to what the comparison needs.
struct Outcome {
    spikes: Spikes,
    /// Final weights.
    weights: Weights,
    /// Mean weight at the end of each second.
    mean_weight: Vec<f64>,
    /// Late events (of events crossing nodes), and the largest 99th percentile of lateness on any link (ms).
    late: Option<(u64, u64, f64)>,
    wall: Duration,
}

fn column<'a, T: 'static>(batch: &'a arrow::record_batch::RecordBatch, name: &str) -> &'a T {
    batch
        .column_by_name(name)
        .unwrap_or_else(|| panic!("column {name}"))
        .as_any()
        .downcast_ref::<T>()
        .expect("column type")
}

/// Spike times (ns) per neuron (actor ID).
type Spikes = HashMap<u64, Vec<u64>>;
/// Weight per synapse (post, index).
type Weights = HashMap<(u64, u32), f32>;

fn read(dir: &Path, seconds: f64) -> (Spikes, Weights, Vec<f64>) {
    let spikes_t = spikes_record::read_table(&dir.join("spikes.arrows")).expect("spikes");
    let mut spikes: HashMap<u64, Vec<u64>> = HashMap::new();
    let (t, neuron) = (
        column::<UInt64Array>(&spikes_t, "t_ns"),
        column::<UInt64Array>(&spikes_t, "neuron"),
    );
    for i in 0..t.len() {
        spikes.entry(neuron.value(i)).or_default().push(t.value(i));
    }
    for list in spikes.values_mut() {
        list.sort_unstable();
    }

    let synapses = spikes_record::read_table(&dir.join("synapses.arrow")).expect("synapses");
    let (post, index, weight) = (
        column::<UInt64Array>(&synapses, "post"),
        column::<UInt32Array>(&synapses, "synapse"),
        column::<Float32Array>(&synapses, "weight"),
    );
    let mut weights: HashMap<(u64, u32), f32> = (0..post.len())
        .map(|i| ((post.value(i), index.value(i)), weight.value(i)))
        .collect();
    let changes = spikes_record::read_table(&dir.join("weights.arrows")).expect("weights");
    let (t, post, index, after) = (
        column::<UInt64Array>(&changes, "t_ns"),
        column::<UInt64Array>(&changes, "post"),
        column::<UInt32Array>(&changes, "synapse"),
        column::<Float32Array>(&changes, "after"),
    );
    let mut order: Vec<usize> = (0..t.len()).collect();
    order.sort_by_key(|&i| t.value(i));
    let mut sum: f64 = weights.values().map(|&w| f64::from(w)).sum();
    let count = weights.len().max(1) as f64;
    let mut mean_weight = Vec::new();
    let mut next_second = 1_000_000_000u64;
    for i in order {
        while t.value(i) >= next_second {
            mean_weight.push(sum / count);
            next_second += 1_000_000_000;
        }
        let w = weights.entry((post.value(i), index.value(i))).or_insert(0.0);
        sum += f64::from(after.value(i)) - f64::from(*w);
        *w = after.value(i);
    }
    while (mean_weight.len() as f64) < seconds.floor() {
        mean_weight.push(sum / count);
    }
    (spikes, weights, mean_weight)
}

fn agents() -> Vec<String> {
    (0..2)
        .map(|_| {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = listener.local_addr().expect("addr").to_string();
            std::thread::spawn(move || spikes_session::cluster::serve_node(listener));
            addr
        })
        .collect()
}

/// Poisson drive: rate per neuron (Hz) and size of each input (mV).
#[derive(Copy, Clone)]
struct Drive {
    hz: f64,
    mv: f32,
}

fn run(
    engine: &Engine,
    description: &NetworkDescription,
    v: &Variant,
    nodes: &[String],
    seconds: f64,
    drive: Drive,
    dir: &Path,
) -> Outcome {
    let _ = std::fs::remove_dir_all(dir);
    let session = engine.create_described(
        v.name,
        format!("// validation network ({})", v.name),
        description.clone(),
    );
    session
        .set_nodes(if v.nodes { nodes.to_vec() } else { Vec::new() })
        .expect("nodes");
    session.set_lockstep(v.lockstep);
    let network = session
        .build(BuildSettings {
            seed: v.seed,
            shards: 1,
            jitter: v.jitter,
            ..BuildSettings::default()
        })
        .expect("build");
    let all: Vec<u32> = (0..network.neurons.len() as u32).collect();
    session
        .add_input(InputSpec::Poisson {
            targets: all.clone(),
            rate_hz: drive.hz * all.len() as f64,
            mv: drive.mv,
            duration: None,
        })
        .expect("input");
    let notices = session.notices();
    let started = Instant::now();
    session
        .start(RunSettings {
            speed: v.speed,
            duration: Some(SimDuration::from_secs_f64(seconds)),
            record: Some(dir.to_path_buf()),
        })
        .expect("start");
    assert!(
        session.wait_for_run(Duration::from_secs(600)),
        "{}: the run should end",
        v.name
    );
    let wall = started.elapsed();
    assert_eq!(session.failure(), None, "{}", v.name);
    assert!(
        notices
            .try_iter()
            .any(|n| matches!(n, SessionNotice::RunEnded { recording: Some(_), .. })),
        "{}: recorded",
        v.name
    );
    let late = session.cluster_status().filter(|s| !s.lockstep).map(|s| {
        let events: u64 = s.links.iter().map(|l| l.2.events).sum();
        let late: u64 = s.links.iter().map(|l| l.2.late).sum();
        let p99 = s
            .links
            .iter()
            .map(|l| l.2.p99_late.as_secs_f64() * 1e3)
            .fold(0.0, f64::max);
        (late, events, p99)
    });
    engine.close(session.id()).expect("close");
    let (spikes, weights, mean_weight) = read(dir, seconds);
    Outcome {
        spikes,
        weights,
        mean_weight,
        late,
        wall,
    }
}

// ----- statistics -----

/// Two-sample Kolmogorov–Smirnov distance between two samples.
fn ks(a: &[f64], b: &[f64]) -> f64 {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_by(f64::total_cmp);
    b.sort_by(f64::total_cmp);
    let (mut i, mut j, mut d) = (0usize, 0usize, 0.0f64);
    while i < a.len() && j < b.len() {
        let x = a[i].min(b[j]);
        while i < a.len() && a[i] <= x {
            i += 1;
        }
        while j < b.len() && b[j] <= x {
            j += 1;
        }
        d = d.max((i as f64 / a.len() as f64 - j as f64 / b.len() as f64).abs());
    }
    d
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let (ma, mb) = (mean(a), mean(b));
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let (va, vb): (f64, f64) = (
        a.iter().map(|x| (x - ma).powi(2)).sum(),
        b.iter().map(|y| (y - mb).powi(2)).sum(),
    );
    cov / (va * vb).sqrt().max(f64::MIN_POSITIVE)
}

struct Summary {
    rate_hz: f64,
    isi_ms: Vec<f64>,
    cv: f64,
}

fn summarize(o: &Outcome, neurons: &[u64], seconds: f64) -> (Summary, Vec<f64>) {
    let rates: Vec<f64> = neurons
        .iter()
        .map(|n| o.spikes.get(n).map_or(0, Vec::len) as f64 / seconds)
        .collect();
    let mut isi_ms = Vec::new();
    let mut cvs = Vec::new();
    for list in o.spikes.values() {
        let isis: Vec<f64> = list.windows(2).map(|w| (w[1] - w[0]) as f64 / 1e6).collect();
        if isis.len() >= 5 {
            let m = mean(&isis);
            let sd = (isis.iter().map(|x| (x - m).powi(2)).sum::<f64>() / isis.len() as f64).sqrt();
            cvs.push(sd / m);
        }
        isi_ms.extend(isis);
    }
    (
        Summary {
            rate_hz: mean(&rates),
            isi_ms,
            cv: mean(&cvs),
        },
        rates,
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).map_or(1_000, |s| s.parse().expect("neurons"));
    let seconds: f64 = args.get(2).map_or(10.0, |s| s.parse().expect("seconds"));
    let report_path = args.get(3).filter(|p| !p.is_empty() && *p != "-").map(PathBuf::from);
    let drive = Drive {
        hz: args.get(4).map_or(10.0, |s| s.parse().expect("drive Hz")),
        mv: args.get(5).map_or(8.0, |s| s.parse().expect("drive mV")),
    };
    let root = std::env::temp_dir().join("spikes-validation");
    let description = network(n);
    let nodes = agents();
    let engine = Engine::new();

    let mut variants = vec![
        Variant {
            name: "A",
            what: "lockstep, real time (reference)",
            nodes: true,
            lockstep: true,
            speed: Speed::Factor(1.0),
            seed: 1,
            jitter: Jitter::None,
        },
        Variant {
            name: "A'",
            what: "lockstep, as fast as possible",
            nodes: true,
            lockstep: true,
            speed: Speed::AsFastAsPossible,
            seed: 1,
            jitter: Jitter::None,
        },
        Variant {
            name: "B",
            what: "best effort, real time",
            nodes: true,
            lockstep: false,
            speed: Speed::Factor(1.0),
            seed: 1,
            jitter: Jitter::None,
        },
        Variant {
            name: "C",
            what: "best effort, 2× real time",
            nodes: true,
            lockstep: false,
            speed: Speed::Factor(2.0),
            seed: 1,
            jitter: Jitter::None,
        },
    ];
    let mut outcomes = Vec::new();
    for v in &variants {
        eprintln!("running {} ({})…", v.name, v.what);
        outcomes.push(run(
            &engine,
            &description,
            v,
            &nodes,
            seconds,
            drive,
            &root.join(v.name),
        ));
    }
    // Intentional jitter about the size of best effort's lateness at real time (σ = its p99, at least 0.2 ms).
    let sigma_ms = outcomes[2].late.map_or(0.5, |(_, _, p99)| p99).max(0.2);
    let extra = [
        Variant {
            name: "D",
            what: "one process, intentional jitter",
            nodes: false,
            lockstep: true,
            speed: Speed::AsFastAsPossible,
            seed: 1,
            jitter: Jitter::Normal {
                sigma: SimDuration::from_millis_f64(sigma_ms),
            },
        },
        Variant {
            name: "E",
            what: "one process, another seed",
            nodes: false,
            lockstep: true,
            speed: Speed::AsFastAsPossible,
            seed: 2,
            jitter: Jitter::None,
        },
    ];
    for v in extra {
        eprintln!("running {} ({})…", v.name, v.what);
        outcomes.push(run(
            &engine,
            &description,
            &v,
            &nodes,
            seconds,
            drive,
            &root.join(v.name),
        ));
        variants.push(v);
    }

    // Every neuron that fired in any run, in a fixed order.
    let mut neurons: Vec<u64> = outcomes.iter().flat_map(|o| o.spikes.keys().copied()).collect();
    neurons.sort_unstable();
    neurons.dedup();
    let summaries: Vec<(Summary, Vec<f64>)> = outcomes.iter().map(|o| summarize(o, &neurons, seconds)).collect();
    let reference = &outcomes[0];
    let identical = outcomes[1].spikes == reference.spikes && outcomes[1].weights == reference.weights;

    let mut out = String::new();
    let line = |out: &mut String, s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    line(
        &mut out,
        format!(
            "Network: {n} neurons (20% inhibitory), {} synapses with STDP, Poisson drive {} Hz × {} mV per neuron; {seconds} s simulated per run; 3 nodes (1 shard each) over loopback TCP.\n",
            n * 20,
            drive.hz,
            drive.mv
        ),
    );
    line(
        &mut out,
        format!(
            "A' (lockstep, as fast as possible) is **{}** to A, spike for spike and weight for weight.\n",
            if identical { "identical" } else { "NOT identical" }
        ),
    );
    line(&mut out, "| Run | What | Wall time | Late events | Mean rate (Hz) | Rate correlation with A | Mean \\|Δrate\\| (Hz) | ISI KS distance to A | CV of ISI | Final weight KS distance to A | Max \\|Δ mean weight\\| over time |".into());
    line(&mut out, "|---|---|---|---|---|---|---|---|---|---|---|".into());
    let (ref_summary, ref_rates) = &summaries[0];
    for ((v, o), (s, rates)) in variants.iter().zip(&outcomes).zip(&summaries) {
        let late = match o.late {
            Some((late, events, p99)) => format!(
                "{:.2}% of {events} (p99 {p99:.2} ms)",
                100.0 * late as f64 / events.max(1) as f64
            ),
            None => "—".into(),
        };
        let mean_dr = mean(
            &rates
                .iter()
                .zip(ref_rates)
                .map(|(a, b)| (a - b).abs())
                .collect::<Vec<_>>(),
        );
        let final_w = |o: &Outcome| {
            let mut w: Vec<f64> = o.weights.values().map(|&w| f64::from(w)).collect();
            w.sort_by(f64::total_cmp);
            w
        };
        let dw = o
            .mean_weight
            .iter()
            .zip(&reference.mean_weight)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        line(
            &mut out,
            format!(
                "| {} | {} | {:.1} s | {late} | {:.3} | {:.4} | {:.4} | {:.4} | {:.3} | {:.4} | {:.5} |",
                v.name,
                v.what,
                o.wall.as_secs_f64(),
                s.rate_hz,
                correlation(rates, ref_rates),
                mean_dr,
                ks(&s.isi_ms, &ref_summary.isi_ms),
                s.cv,
                ks(&final_w(o), &final_w(reference)),
                dw
            ),
        );
    }
    line(
        &mut out,
        format!("\nD's jitter: normal, σ = {sigma_ms:.2} ms on every connection delay."),
    );
    print!("{out}");
    if let Some(path) = report_path {
        std::fs::write(&path, out).expect("write report");
        eprintln!("wrote {}", path.display());
    }
}
