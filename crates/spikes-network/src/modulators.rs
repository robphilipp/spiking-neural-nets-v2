//! Neuromodulators in a running network: releases, the shard-local field that records them, and the per-neuron
//! view that turns them into concentrations (actors plan 09 §2).
//!
//! A release at time `t` and position `x` reaches a neuron at distance `d` (if `d ≤ radius`) at
//! `t + latency + d/speed`, with amplitude `amount·exp(−d/falloff)`, then decays with the clearance time constant.
//! Every shard holds the releases in a [`ModulatorField`]; neurons evaluate concentrations lazily, at their own
//! events, from their own position. Nothing ticks.
//!
//! Releases are kept for [`RETENTION_MS`] (or 20 clearance times, if longer). Three-factor learning integrates over
//! the time since a synapse's last update, so a synapse idle for longer than that loses the contribution of
//! releases that have been dropped.

use std::sync::Arc;

use spikes_dna::ModulatorDescription;
use spikes_model::modulation::{pulse_integral, pulse_level};
use spikes_model::{Modulators, Pulse};
use tempo_runtime::{Actor, ActorId, Ctx, Field, SimDuration, SimTime, SpatialBroadcast};

use crate::messages::{Event, ModulatorMsg, NeuronMsg, SimMsg};

/// How long releases are kept, at least, in ms.
pub const RETENTION_MS: f64 = 60_000.0;

/// A modulator's spreading and clearance.
#[derive(Clone, Debug, PartialEq)]
pub struct ModulatorKind {
    /// Name.
    pub name: String,
    /// Clearance time constant in ms.
    pub clearance_ms: f64,
    /// Reach in µm.
    pub radius_um: f64,
    /// Spreading speed in µm/ms (= mm/s).
    pub speed_um_per_ms: f64,
    /// Delay from release to the start of spreading.
    pub latency: SimDuration,
    /// Falloff length in µm.
    pub falloff_um: Option<f64>,
    /// Whether any receptor has a direct-potential effect (then releases also wake the neurons they reach).
    pub direct: bool,
}

impl ModulatorKind {
    /// From a parsed description.
    pub fn from_description(d: &ModulatorDescription, direct: bool) -> Self {
        Self {
            name: d.name.clone(),
            clearance_ms: d.clearance_ms,
            radius_um: d.radius_um,
            speed_um_per_ms: d.speed_m_per_s * 1e3,
            latency: SimDuration::from_millis_f64(d.latency_ms),
            falloff_um: d.falloff_um,
            direct,
        }
    }

    /// When and how strongly a release at `center` at time `at` reaches `position`.
    fn pulse(&self, release: &ReleaseEvent, position: [f64; 3]) -> Option<Pulse> {
        let d = distance(release.center, position);
        if d > self.radius_um {
            return None;
        }
        let travel = if self.speed_um_per_ms.is_finite() {
            SimDuration::from_millis_f64(d / self.speed_um_per_ms)
        } else {
            SimDuration::ZERO
        };
        let falloff = self.falloff_um.map_or(1.0, |len| (-d / len).exp());
        Some(Pulse {
            arrival: release.at + self.latency + travel,
            amplitude: release.amount * falloff,
        })
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(&b).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt()
}

/// One release.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ReleaseEvent {
    /// Which modulator.
    pub kind: u16,
    /// When it was released.
    pub at: SimTime,
    /// Where, in µm.
    pub center: [f64; 3],
    /// How much.
    pub amount: f64,
}

/// The releases every shard knows about.
#[derive(Clone, Debug)]
pub struct ModulatorField {
    kinds: Arc<Vec<ModulatorKind>>,
    releases: Vec<ReleaseEvent>,
}

impl ModulatorField {
    /// A field for these modulators, with no releases yet.
    pub fn new(kinds: Arc<Vec<ModulatorKind>>) -> Self {
        Self {
            kinds,
            releases: Vec::new(),
        }
    }

    /// The modulators.
    pub fn kinds(&self) -> &[ModulatorKind] {
        &self.kinds
    }

    /// Releases currently retained.
    pub fn releases(&self) -> &[ReleaseEvent] {
        &self.releases
    }

    /// The concentrations seen from `position`.
    pub fn at(&self, position: [f64; 3]) -> FieldView<'_> {
        FieldView { field: self, position }
    }

    fn pulses(&self, kind: u16, position: [f64; 3]) -> impl Iterator<Item = Pulse> + '_ {
        let k = &self.kinds[usize::from(kind)];
        self.releases
            .iter()
            .filter(move |r| r.kind == kind)
            .filter_map(move |r| k.pulse(r, position))
    }
}

impl Field for ModulatorField {
    type Update = ReleaseEvent;

    fn apply(&mut self, at: SimTime, release: ReleaseEvent) {
        let kinds = Arc::clone(&self.kinds);
        self.releases.retain(|r| {
            let keep_ms = (20.0 * kinds[usize::from(r.kind)].clearance_ms).max(RETENTION_MS);
            at.saturating_duration_since(r.at).as_millis_f64() < keep_ms
        });
        self.releases.push(release);
    }
}

/// Modulator concentrations at one position.
#[derive(Copy, Clone, Debug)]
pub struct FieldView<'a> {
    field: &'a ModulatorField,
    position: [f64; 3],
}

impl Modulators for FieldView<'_> {
    fn level(&self, kind: u16, t: SimTime) -> f64 {
        pulse_level(self.field.pulses(kind, self.position), self.clearance_ms(kind), t)
    }

    fn decayed_integral(&self, kind: u16, t0: SimTime, t1: SimTime, tau_ms: f64) -> f64 {
        pulse_integral(
            self.field.pulses(kind, self.position),
            self.clearance_ms(kind),
            t0,
            t1,
            tau_ms,
        )
    }

    fn clearance_ms(&self, kind: u16) -> f64 {
        self.field.kinds[usize::from(kind)].clearance_ms
    }
}

/// Releases `amount` of modulator `kind` at `center` at time `at` (now or later, e.g. a spike after its rise
/// time): every shard's field learns of it after the modulator's latency, and, for modulators with
/// direct-potential receptors, the neurons it reaches are woken on arrival.
pub(crate) fn release(
    ctx: &mut Ctx<'_, SimMsg, Event>,
    kinds: &[ModulatorKind],
    kind: u16,
    amount: f64,
    center: [f64; 3],
    at: SimTime,
) {
    let Some(k) = kinds.get(usize::from(kind)) else {
        return;
    };
    let wait = at.saturating_duration_since(ctx.now()) + k.latency;
    ctx.update_field::<ModulatorField>(
        wait,
        ReleaseEvent {
            kind,
            at,
            center,
            amount,
        },
    );
    if k.direct {
        ctx.broadcast_spatial(
            SpatialBroadcast {
                center,
                radius: k.radius_um,
                base_delay: wait,
                speed: k.speed_um_per_ms * 1e3, // position units (µm) per second
            },
            SimMsg::Neuron(NeuronMsg::Modulate),
        );
    }
}

/// A release site that isn't a neuron, e.g. a "reward" button in the UI: send it
/// [`ModulatorMsg::Release`] (with `Control::inject_live`) to release a modulator at its position.
#[derive(Clone, Debug)]
pub struct ModulatorRelease {
    /// Where releases happen, in µm.
    pub position: [f64; 3],
    /// The network's modulators.
    pub kinds: Arc<Vec<ModulatorKind>>,
}

impl Actor<SimMsg, Event> for ModulatorRelease {
    fn on_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, msg: SimMsg) {
        if let SimMsg::Modulator(ModulatorMsg::Release { kind, amount }) = msg {
            let now = ctx.now();
            release(ctx, &self.kinds, kind, f64::from(amount), self.position, now);
        }
    }
}
