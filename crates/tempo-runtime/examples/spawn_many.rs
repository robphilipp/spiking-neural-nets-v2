//! Spawns many 64-byte actors and reports memory use. Phase 1 target: 10 million in under 2 GB.
//!
//! ```sh
//! cargo run --release -p tempo-runtime --example spawn_many -- 10000000
//! ```

use std::time::Instant;

use tempo_runtime::{Actor, ActorId, Ctx, Runtime, RuntimeConfig, SimTime};

/// A 64-byte actor, roughly the size of a compact neuron's hot state.
struct Neuron64 {
    state: [u64; 8],
}

impl Actor<u64, ()> for Neuron64 {
    fn on_message(&mut self, _ctx: &mut Ctx<'_, u64, ()>, _from: ActorId, msg: u64) {
        self.state[0] = self.state[0].wrapping_add(msg);
    }
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .map_or(10_000_000, |s| s.parse().expect("actor count"));
    let mut rt: Runtime<u64, ()> = Runtime::new(RuntimeConfig::default());

    let start = Instant::now();
    let mut last = ActorId::WORLD;
    for i in 0..n {
        last = rt.spawn(Neuron64 { state: [i as u64; 8] });
    }
    let spawned = start.elapsed();

    // Run once: storage is trimmed to fit, every on_start executes, and the last actor gets a message.
    rt.inject(last, SimTime::ZERO, 1);
    rt.run();

    let bytes = rt.memory_bytes();
    println!("actors:          {n}");
    println!("actor size:      {} bytes", size_of::<Neuron64>());
    println!(
        "spawn time:      {spawned:.2?} ({:.0} ns/actor)",
        spawned.as_nanos() as f64 / n as f64
    );
    println!(
        "runtime memory:  {:.2} GB ({:.0} bytes/actor)",
        bytes as f64 / 1e9,
        bytes as f64 / n as f64
    );
    println!("started:         {}", rt.stats().started);
}
