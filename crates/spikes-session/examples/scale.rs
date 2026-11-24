//! Phase 6: how big a network fits, and how fast it runs, on one machine.
//!
//! ```text
//! cargo run --release -p spikes-session --example scale -- [neurons] [synapses per neuron] [seconds] [shards]
//! ```
//!
//! A random network (neurons in a cube, each with `k` random inputs, 20% inhibitory), driven at ~5 Hz. Reports the
//! time and memory of each stage, then runs as fast as possible and reports throughput.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Counts live heap bytes (the process's resident size keeps freed pages, so it can't show savings).
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator, only counting.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            let now = if new_size >= layout.size() {
                LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed) + new_size - layout.size()
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed) - (layout.size() - new_size)
            };
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn gb(bytes: usize) -> f64 {
    bytes as f64 / 1e9
}

use spikes_session::{BuildSettings, Engine, InputSpec, RunSettings, Speed};
use tempo_runtime::SimDuration;

/// Resident memory of this process, in GB (from `ps`).
fn rss_gb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
        / 1024.0
}

fn stage(name: &str, t: Instant) {
    println!(
        "{name:<28} {:>8.2} s   heap {:>6.2} GB (peak {:>6.2})   RSS {:>6.2} GB",
        t.elapsed().as_secs_f64(),
        gb(LIVE.load(Ordering::Relaxed)),
        gb(PEAK.load(Ordering::Relaxed)),
        rss_gb()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).map_or(200_000, |s| s.parse().expect("neurons"));
    let k: usize = args.get(2).map_or(100, |s| s.parse().expect("synapses per neuron"));
    let seconds: f64 = args.get(3).map_or(1.0, |s| s.parse().expect("seconds"));
    let shards: usize = args.get(4).map_or(8, |s| s.parse().expect("shards"));
    println!("{n} neurons × {k} synapses = {} synapses, {shards} shards", n * k);

    let t = Instant::now();
    // Populations and a rule: nothing is listed connection by connection.
    let side = 1000.0 * (n as f64 / 100_000.0).cbrt(); // constant density
    let (exc, inh) = (n - n / 5, n / 5);
    let w = 20.0 / k as f64; // keeps the total input per neuron about constant
    let neuron = "mst=10 mV, mpn=0 mV, mpd=20 ms, rfp=4 ms, spp=1 mV, csp=1 m/s";
    let region = format!("REG=(cst=bx, px1=0, px2=0, px3=0, sx1={side}, sx2={side}, sx3={side})");
    let text = format!(
        "(POP=[(nam=exc, cnt={exc}, {neuron}, {region}), (nam=inh, cnt={inh}, inh=t, {neuron}, {region}, sed=2)],\n \
         RUL=[(prp=default, psp=default, cnt={k}, cnw={w})])"
    );
    let d = spikes_dna::parse(&text).expect("description parses");
    stage("description generated", t);

    let engine = Engine::new();
    let session = engine.create_described("scale", text, d);
    let t = Instant::now();
    let network = session
        .build(BuildSettings {
            shards,
            record_weights: false,
            ..BuildSettings::default()
        })
        .expect("build");
    stage("built", t);
    println!(
        "  {:.0} bytes of heap per synapse",
        LIVE.load(Ordering::Relaxed) as f64 / network.synapse_count as f64
    );
    println!(
        "  {} synapses; the summary lists {}",
        network.synapse_count,
        network.synapses.len()
    );
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
    let spikes: u64 = (0..n as u32)
        .step_by(n / 1000)
        .map(|i| session.query_neuron(i).expect("query").spikes)
        .sum::<u64>()
        * (n / 1000) as u64;
    println!(
        "  ~{:.1} Hz per neuron; {:.2} s wall per simulated second; ~{:.1} M synaptic events per wall second",
        spikes as f64 / n as f64 / seconds,
        wall / seconds,
        spikes as f64 * k as f64 / wall / 1e6
    );
}
