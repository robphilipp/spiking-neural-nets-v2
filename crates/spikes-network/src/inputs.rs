//! Input actors: drive neurons from inside the simulation. (Live input from a UI or hardware arrives through
//! `Control::inject_live` instead.)
//!
//! Inputs are direct messages with a delay, so with several shards the runtime's `min_remote_delay` must be at
//! most that delay.

use rand::RngExt;
use tempo_runtime::{Actor, ActorId, Ctx, SimDuration, SimTime};

use crate::messages::{Event, InputMsg, NeuronMsg, NeuronRef, SimMsg, Tell};

/// Sends `mv` to one random target at Poisson-distributed times, from `start` until `until`.
/// The delay of the messages synthetic inputs send to their neurons.
pub const INPUT_DELAY: SimDuration = SimDuration::from_millis(1);

/// The shortest delay of any *direct* message a network built from `description` sends between neurons (inputs'
/// messages, modulator releases): what the runtime's `min_remote_delay` must be at most. Connections don't count
/// (their delays are known in advance).
pub fn direct_delay(description: &spikes_dna::NetworkDescription) -> SimDuration {
    description
        .modulators
        .iter()
        .map(|m| SimDuration::from_millis_f64(m.latency_ms))
        .fold(INPUT_DELAY, SimDuration::min)
}

#[derive(Clone, Debug)]
pub struct PoissonInput {
    /// Mean rate in Hz.
    pub rate_hz: f64,
    /// Neurons to choose from.
    pub targets: Vec<NeuronRef>,
    /// Signal strength in mV.
    pub mv: f32,
    /// Delay from the event to its arrival (afferent conduction).
    pub delay: SimDuration,
    /// When to start.
    pub start: SimTime,
    /// When to stop.
    pub until: SimTime,
}

impl PoissonInput {
    fn schedule_next(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, from: SimTime) {
        let u: f64 = 1.0 - ctx.rng().random::<f64>();
        let wait = SimDuration::from_secs_f64(-u.ln() / self.rate_hz).max(SimDuration::NANOSECOND);
        let at = from + wait;
        if at <= self.until {
            ctx.send_at(ctx.me(), at, SimMsg::Input(InputMsg::Tick));
        }
    }
}

impl Actor<SimMsg, Event> for PoissonInput {
    fn on_start(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>) {
        let from = self.start.max(ctx.now());
        self.schedule_next(ctx, from);
    }

    fn on_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, msg: SimMsg) {
        if msg == SimMsg::Input(InputMsg::Tick) && !self.targets.is_empty() {
            let target = self.targets[ctx.rng().random_range(0..self.targets.len())];
            ctx.tell(target, self.delay, NeuronMsg::External { mv: self.mv });
            let now = ctx.now();
            self.schedule_next(ctx, now);
        }
    }
}

/// Sends `mv` to every target every `period`, from `start` until `until`.
#[derive(Clone, Debug)]
pub struct PeriodicInput {
    /// Interval between pulses.
    pub period: SimDuration,
    /// Neurons to stimulate.
    pub targets: Vec<NeuronRef>,
    /// Signal strength in mV.
    pub mv: f32,
    /// Delay from the pulse to its arrival.
    pub delay: SimDuration,
    /// First pulse.
    pub start: SimTime,
    /// No pulses after this.
    pub until: SimTime,
}

impl Actor<SimMsg, Event> for PeriodicInput {
    fn on_start(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>) {
        if self.start <= self.until {
            let at = self.start.max(ctx.now());
            ctx.send_at(ctx.me(), at, SimMsg::Input(InputMsg::Tick));
        }
    }

    fn on_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, msg: SimMsg) {
        if msg != SimMsg::Input(InputMsg::Tick) {
            return;
        }
        for &target in &self.targets {
            ctx.tell(target, self.delay, NeuronMsg::External { mv: self.mv });
        }
        let next = ctx.now() + self.period;
        if next <= self.until {
            ctx.send_at(ctx.me(), next, SimMsg::Input(InputMsg::Tick));
        }
    }
}

/// Sends `mv` to every target at each of `times` (ascending).
#[derive(Clone, Debug)]
pub struct PulseInput {
    /// Neurons to stimulate.
    pub targets: Vec<NeuronRef>,
    /// When (ascending).
    pub times: Vec<SimTime>,
    /// Signal strength in mV.
    pub mv: f32,
    /// Delay from the pulse to its arrival.
    pub delay: SimDuration,
}

impl PulseInput {
    /// Schedules the first pulse at or after `from`.
    fn schedule(&self, ctx: &mut Ctx<'_, SimMsg, Event>, from: SimTime, strictly_after: bool) {
        let next = self
            .times
            .iter()
            .find(|&&t| if strictly_after { t > from } else { t >= from });
        if let Some(&at) = next {
            ctx.send_at(ctx.me(), at, SimMsg::Input(InputMsg::Tick));
        }
    }
}

impl Actor<SimMsg, Event> for PulseInput {
    fn on_start(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>) {
        let now = ctx.now();
        self.schedule(ctx, now, false);
    }

    fn on_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, msg: SimMsg) {
        if msg != SimMsg::Input(InputMsg::Tick) {
            return;
        }
        // (Pulses at the same time are one pulse.)
        for &target in &self.targets {
            ctx.tell(target, self.delay, NeuronMsg::External { mv: self.mv });
        }
        let now = ctx.now();
        self.schedule(ctx, now, true);
    }
}

/// A signal computed in simulated time: every `sample`, evaluates `formula` for each channel (`t` in ms since
/// `start`, `c`, `n`), encodes the value, and sends the resulting input to the channel's neurons. Deterministic: the
/// formula's noise comes from `key` (the run's seed and the source's name), the rate encoder's from the actor's
/// random stream.
#[derive(Clone, Debug)]
pub struct SignalInput {
    /// Each channel's neurons.
    pub channels: Vec<Vec<NeuronRef>>,
    /// The formula.
    pub formula: spikes_signal::Formula,
    /// How samples become input.
    pub encoding: spikes_signal::SampleEncoding,
    /// Time between samples.
    pub sample: SimDuration,
    /// The noise key (`spikes_signal::noise::key(seed, name)`).
    pub key: u64,
    /// Delay from an event to its arrival.
    pub delay: SimDuration,
    /// The first sample.
    pub start: SimTime,
    /// No samples after this.
    pub until: SimTime,
    /// The data files the formula looks up.
    pub data: spikes_signal::DataSets,
    /// Each channel's encoder (filled on start).
    pub encoders: Vec<spikes_signal::Channel>,
    /// The next sample's index.
    pub next: u64,
}

impl SignalInput {
    /// The time of sample `index`.
    fn time_of(&self, index: u64) -> SimTime {
        self.start
            .saturating_add(SimDuration::from_nanos(self.sample.as_nanos().saturating_mul(index)))
    }
}

impl Actor<SimMsg, Event> for SignalInput {
    fn on_start(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>) {
        if self.encoders.len() != self.channels.len() {
            let rng = ctx.rng();
            self.encoders = (0..self.channels.len())
                .map(|_| spikes_signal::Channel::new(&mut || rng.random::<f64>()))
                .collect();
        }
        // (Resuming from a snapshot, the pending tick is restored with it.)
        if self.next == 0 && self.start <= self.until {
            let at = self.start.max(ctx.now());
            ctx.send_at(ctx.me(), at, SimMsg::Input(InputMsg::Tick));
        }
    }

    fn on_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, msg: SimMsg) {
        if msg != SimMsg::Input(InputMsg::Tick) {
            return;
        }
        let index = self.next;
        let t_ms = self.sample.as_millis_f64() * index as f64;
        let dt_s = self.sample.as_secs_f64();
        let n = self.channels.len() as u32;
        let at = spikes_signal::At {
            sample: index,
            key: self.key,
        };
        for c in 0..self.channels.len() {
            let v = self.formula.eval_with(t_ms, c as u32, n, at, &self.data) as f32;
            let mut events: Vec<(f64, f32)> = Vec::new();
            let rng = ctx.rng();
            self.encoders[c].encode(
                self.encoding,
                v,
                dt_s,
                &mut || rng.random::<f64>(),
                &mut |offset, mv| {
                    events.push((offset, mv));
                },
            );
            for (offset, mv) in events {
                let delay = self.delay.saturating_add(SimDuration::from_secs_f64(offset.max(0.0)));
                for &target in &self.channels[c] {
                    ctx.tell(target, delay, NeuronMsg::External { mv });
                }
            }
        }
        self.next = index + 1;
        let next = self.time_of(self.next);
        if next <= self.until && next < SimTime::MAX {
            ctx.send_at(ctx.me(), next, SimMsg::Input(InputMsg::Tick));
        }
    }
}
