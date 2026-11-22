# API sketch (illustrative, not final)

## Core types

```rust
#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd, Hash, Debug)]
pub struct SimTime(u64);            // ns since sim start
#[derive(Copy, Clone, Eq, Ord, PartialEq, PartialOrd, Hash, Debug)]
pub struct SimDuration(u64);        // ns
impl SimDuration { pub fn from_micros_f64(us: f64) -> Self; pub fn from_millis_f64(ms: f64) -> Self; /* … */ }

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct ActorId(u64);
impl ActorId { pub const WORLD: ActorId = ActorId(u64::MAX); }

#[derive(Copy, Clone, Debug)]
pub struct ActorRef { id: ActorId, loc: Location }

pub struct RouteId(u32);
```

## Actor trait

```rust
pub trait Actor<M, E>: Send + 'static {
    /// Called once when the simulation starts (at SimTime 0, or at spawn time for runtime spawns).
    fn on_start(&mut self, _ctx: &mut Ctx<'_, M, E>) {}

    /// Called for each data-plane message, in (at, src, seq) order. ctx.now() == message time.
    fn on_message(&mut self, ctx: &mut Ctx<'_, M, E>, from: ActorId, msg: M);

    /// Called for each message expanded from a fan-out route; `edge` indexes the receiver's per-edge state.
    fn on_route_message(&mut self, ctx: &mut Ctx<'_, M, E>, from: ActorId, edge: EdgeIdx, msg: &M) {
        let _ = (ctx, from, edge, msg); unimplemented!("actor does not accept routed messages")
    }

    /// Control-plane query, answered at a window boundary.
    fn on_query(&mut self, _q: &dyn Any) -> Option<Box<dyn Any + Send>> { None }
}
```

## Context (what an actor can do while handling a message)

```rust
impl<'a, M, E> Ctx<'a, M, E> {
    pub fn me(&self) -> ActorId;
    pub fn now(&self) -> SimTime;                          // == current message timestamp
    pub fn send(&mut self, to: impl Into<ActorRef>, msg: M);                    // at = now (local only unless L == 0)
    pub fn send_after(&mut self, to: impl Into<ActorRef>, d: SimDuration, msg: M);
    pub fn send_at(&mut self, to: impl Into<ActorRef>, at: SimTime, msg: M);   // at >= now
    pub fn fanout(&mut self, route: RouteId, msg: M) where M: Clone;           // one-to-many, per-edge delays
    pub fn fanout_at(&mut self, route: RouteId, base: SimTime, msg: M) where M: Clone; // base >= now; edge delays added
    pub fn schedule_self(&mut self, d: SimDuration, msg: M);
    pub fn rng(&mut self) -> &mut impl rand::RngCore;      // deterministic per-actor stream
    pub fn emit(&mut self, event: E);                      // observability tap (never blocks)
}
```

## Building and running

```rust
let mut rt = Runtime::<SimMsg, SimEvent>::builder()
    .shards(num_cpus::get_physical())
    .seed(42)
    .queue(QueueKind::TimingWheel { bucket: SimDuration::from_micros(1) })
    .cluster_sync(ClusterSync::RealTimeBestEffort { batch_every: Duration::from_micros(100), run_ahead: true })
    // or ClusterSync::Strict for reproducible runs
    .register_kind::<Neuron>()
    .register_kind::<PoissonInput>()
    .build()?;

let ids = rt.control().spawn_batch(neurons.iter().map(|n| (Placement::Near(n.location), Neuron::from(n))))?;
rt.control().add_route(pre, edges /* [(post_id, delay, edge_idx)] */)?;

let mut spikes = rt.taps().subscribe(TapFilter::all(), TapMode::Batched { every: Duration::from_millis(16) });

rt.control().start(Clock::RealTime { time_factor: 1.0 })?;
rt.control().set_time_factor(10.0)?;     // slow down 10× (effective on all nodes ~20–50 ms later)
rt.control().pause()?; rt.control().step(SimTime::from_millis(5))?; rt.control().resume()?;
let lat = rt.metrics().cross_host_lateness(); // late fraction, p50/p99 per link, for the UI
// … UI pulls from `spikes`, sends inject() …
rt.control().stop()?;
```

## Neuron on top of it (sketch)

```rust
pub enum SimMsg { Spike { amplitude_mv: f32 }, TonicTick { gen: u32 }, External { mv: f32 } }

pub struct Neuron {
    params: NeuronParams,        // shared via Arc or an index into a params table to keep this struct small
    state: MembraneState,        // u, b, last_event, last_fire, ...
    synapses: Vec<Synapse>,      // indexed by EdgeIdx (incoming); weight, eq_weight, release state
    out_route: RouteId,          // all outgoing edges with delays = distance/csp
    tonic_gen: u32,
}

impl Actor<SimMsg, SimEvent> for Neuron {
    fn on_route_message(&mut self, ctx: &mut Ctx<'_, SimMsg, SimEvent>, from: ActorId, edge: EdgeIdx, msg: &SimMsg) {
        let SimMsg::Spike { amplitude_mv } = *msg else { return };
        // pure model code, no framework types inside:
        let out = model::on_signal(&self.params, &mut self.state, &mut self.synapses[edge.0], ctx.now(), amplitude_mv, ctx.rng());
        if let Some(fire) = out.fired {
            ctx.emit(SimEvent::Spike { id: ctx.me(), t: ctx.now() + fire.rise_delay });
            // rise delay is per-spike, route delays are per-edge: send at now + rise; route adds its edge delays
            ctx.fanout_at(self.out_route, ctx.now() + fire.rise_delay, SimMsg::Spike { amplitude_mv: self.params.spike_mv });
        }
    }
    fn on_message(&mut self, ctx: &mut Ctx<'_, SimMsg, SimEvent>, _from: ActorId, msg: SimMsg) {
        match msg {
            SimMsg::External { mv } => { /* world signal, weight 1 */ }
            SimMsg::TonicTick { gen } if gen == self.tonic_gen => { /* bistable: fire, maybe reschedule */ }
            _ => {}
        }
    }
}
```

Notes:
- `fanout_at` (send a route at a future base time) is needed for the rise delay. It respects the intra-node
  lookahead because `base ≥ now` and `edge delay ≥ L_node`.
- With release state on the receiving side, `on_route_message` first updates that synapse's facilitation, then
  draws the release with `ctx.rng()`. If there's a release, it records the depletion and the signal goes into the
  membrane-potential update. If not, the neuron returns after the facilitation update.
- Optional delay jitter (intentional noise) is applied in route expansion from a per-route distribution.
- Model code (`model::on_signal`) takes plain data and an `RngCore`, so it can be unit-tested against the v1
  equations without the runtime.
