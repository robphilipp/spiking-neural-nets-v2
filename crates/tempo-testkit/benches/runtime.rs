//! Runtime benchmarks. Throughput is reported in delivered events per second.

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use tempo_runtime::{QueueKind, Runtime, RuntimeConfig, SimDuration, SimTime};
use tempo_testkit::models::{lif, pingpong, ring};

fn config(queue: QueueKind) -> RuntimeConfig {
    RuntimeConfig {
        seed: 1,
        queue,
        trace: false,
    }
}

const QUEUES: [(&str, QueueKind); 2] = [
    ("heap", QueueKind::Heap),
    (
        "wheel",
        QueueKind::Wheel {
            bucket: SimDuration::MICROSECOND,
        },
    ),
];

/// Bare per-message cost: one message in flight at a time.
fn bench_pingpong(c: &mut Criterion) {
    const HITS: u64 = 100_000;
    let mut group = c.benchmark_group("pingpong");
    group.throughput(Throughput::Elements(HITS + 1));
    for (name, queue) in QUEUES {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut rt = Runtime::new(config(queue));
                    pingpong::build(&mut rt, SimDuration::from_micros(1), HITS);
                    rt
                },
                |mut rt| rt.run(),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// 10,000 actors with 1,000 tokens in flight, µs-scale irregular delays.
fn bench_ring(c: &mut Criterion) {
    const HOPS: u64 = 1_000;
    const TOKENS: u32 = 1_000;
    let delays: Vec<SimDuration> = (0..10_000)
        .map(|i| SimDuration::from_nanos(5_000 + (i * 7_919) % 50_000))
        .collect();
    let mut group = c.benchmark_group("ring_10k");
    group.sample_size(10);
    group.throughput(Throughput::Elements(u64::from(TOKENS) * (HOPS + 1)));
    for (name, queue) in QUEUES {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut rt = Runtime::new(config(queue));
                    ring::build(&mut rt, &delays, TOKENS, HOPS, false);
                    rt
                },
                |mut rt| rt.run(),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// 10,000 LIF neurons × 100 synapses, 100 ms of simulated time.
fn bench_lif(c: &mut Criterion) {
    let lif_config = lif::LifConfig {
        neurons: 10_000,
        synapses_per_neuron: 100,
        sources: 200,
        ..lif::LifConfig::default()
    };
    let end = SimTime::from_start(SimDuration::from_millis(100));
    let build = |queue| {
        let mut rt = Runtime::new(config(queue));
        lif::build(&mut rt, &lif_config);
        rt
    };
    // Dry run to learn how many events the benchmark delivers.
    let mut dry = build(QUEUES[1].1);
    let events = dry.run_until(end);
    eprintln!(
        "lif_10k: {events} events in 100 ms simulated ({} routed sends)",
        dry.stats().routed_sends
    );

    let mut group = c.benchmark_group("lif_10k_100ms");
    group.sample_size(10);
    group.throughput(Throughput::Elements(events));
    for (name, queue) in QUEUES {
        group.bench_function(name, |b| {
            b.iter_batched(|| build(queue), |mut rt| rt.run_until(end), BatchSize::LargeInput);
        });
    }
    group.finish();
}

criterion_group!(benches, bench_pingpong, bench_ring, bench_lif);
criterion_main!(benches);
