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
