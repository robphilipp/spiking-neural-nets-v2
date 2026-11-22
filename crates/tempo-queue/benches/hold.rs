//! "Hold" benchmark: keep N events queued; each iteration pops the earliest and pushes one new event at
//! `popped time + delay`, with delays drawn from a mix typical of a spiking network.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use tempo_core::{ActorId, OrderKey, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

/// Small deterministic RNG (SplitMix64) so the benchmark doesn't depend on a `rand` version.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo)
    }

    /// 70% synaptic delays (10 µs–2 ms), 25% timer-like (1–50 ms), 5% long (0.1–100 s). In nanoseconds.
    fn delay_ns(&mut self) -> u64 {
        match self.next() % 100 {
            0..70 => self.range(10_000, 2_000_000),
            70..95 => self.range(1_000_000, 50_000_000),
            _ => self.range(100_000_000, 100_000_000_000),
        }
    }
}

fn key(at: u64, seq: u64) -> OrderKey {
    OrderKey {
        at: SimTime::from_nanos(at),
        src: ActorId::from_raw(seq % 1024),
        seq,
        sub: 0,
    }
}

/// A queue pre-filled with `n` events, plus the RNG and sequence counter to continue the hold loop.
fn prefilled<Q: TimeQueue<u64>>(mut q: Q, n: usize) -> (Q, SplitMix, u64) {
    let mut rng = SplitMix(42);
    for seq in 0..n as u64 {
        q.push(key(rng.delay_ns(), seq), seq);
    }
    (q, rng, n as u64)
}

fn hold<Q: TimeQueue<u64>>(state: &mut (Q, SplitMix, u64)) {
    let (q, rng, seq) = state;
    let (k, item) = q.pop().expect("queue is never empty");
    *seq += 1;
    q.push(key(k.at.as_nanos() + rng.delay_ns(), *seq), black_box(item));
}

fn bench_hold(c: &mut Criterion) {
    let mut group = c.benchmark_group("hold");
    group.throughput(Throughput::Elements(1));
    for n in [1_000usize, 100_000, 1_000_000] {
        group.bench_with_input(BenchmarkId::new("heap", n), &n, |b, &n| {
            let mut state = prefilled(HeapQueue::with_capacity(n + 1), n);
            b.iter(|| hold(&mut state));
        });
        group.bench_with_input(BenchmarkId::new("wheel", n), &n, |b, &n| {
            let mut state = prefilled(WheelQueue::new(), n);
            b.iter(|| hold(&mut state));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_hold);
criterion_main!(benches);
