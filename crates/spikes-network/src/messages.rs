//! The simulation's messages and events, and typed actor handles.
//!
//! All actors share one message enum, [`SimMsg`], nested by actor kind (actors plan 07 §2). A typed handle
//! [`Ref<T>`] only accepts its kind's messages, so `ctx.tell(neuron, NeuronMsg::External { .. })` compiles and
//! sending an input actor's message to a neuron doesn't.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use spikes_model::Transition;
use tempo_runtime::{ActorId, Ctx, SimDuration, SimTime, TimerToken};

/// Every data-plane message.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SimMsg {
    /// For a neuron.
    Neuron(NeuronMsg),
    /// For an input actor.
    Input(InputMsg),
    /// For a modulator release site.
    Modulator(ModulatorMsg),
}

/// Messages a modulator release site accepts.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ModulatorMsg {
    /// Release this much of modulator `kind` (its index in the description's `MOD` list).
    Release {
        /// Which modulator.
        kind: u16,
        /// How much.
        amount: f32,
    },
}

impl From<ModulatorMsg> for SimMsg {
    fn from(msg: ModulatorMsg) -> Self {
        Self::Modulator(msg)
    }
}

/// Messages a neuron accepts.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NeuronMsg {
    /// A spike arriving through a synapse (sent along routes), with the sender's amplitude.
    Spike {
        /// Amplitude in mV (the sender's `spp`); the synapse applies its sign and weight.
        mv: f32,
    },
    /// Input from outside the network (a sensor, the UI), applied with weight 1.
    External {
        /// Signal in mV.
        mv: f32,
    },
    /// A wake-up the neuron scheduled for itself (tonic spikes, predicted threshold crossings).
    Wake(TimerToken),
    /// A modulator with a direct-potential effect has just reached the neuron.
    Modulate,
}

/// Messages an input actor accepts.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum InputMsg {
    /// Its own timer.
    Tick,
}

impl From<NeuronMsg> for SimMsg {
    fn from(msg: NeuronMsg) -> Self {
        Self::Neuron(msg)
    }
}

impl From<InputMsg> for SimMsg {
    fn from(msg: InputMsg) -> Self {
        Self::Input(msg)
    }
}

/// What the network reports.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Event {
    /// The neuron fired; `at` includes its rise time (the emitted event's own time is when it was decided).
    Spike {
        /// Spike time.
        at: SimTime,
    },
    /// A bistable neuron started or stopped tonic firing.
    Transition(Transition),
    /// Learning changed a synapse weight (if weight recording is on).
    Weight {
        /// The synapse's index on the receiving (emitting) neuron.
        synapse: u32,
        /// The sending neuron.
        pre: ActorId,
        /// Weight before.
        before: f32,
        /// Weight after.
        after: f32,
    },
    /// The membrane potential after an update (only for observed neurons).
    Membrane {
        /// Potential in mV.
        mv: f32,
    },
}

/// A handle to an actor that accepts messages of type `T`.
pub struct Ref<T> {
    id: ActorId,
    kind: PhantomData<fn(T)>,
}

/// A handle to a neuron.
pub type NeuronRef = Ref<NeuronMsg>;
/// A handle to an input actor.
pub type InputRef = Ref<InputMsg>;
/// A handle to a modulator release site.
pub type ModulatorRef = Ref<ModulatorMsg>;

impl<T> Ref<T> {
    /// Wraps an ID. Only do this for an ID you know belongs to an actor accepting `T`.
    pub const fn new(id: ActorId) -> Self {
        Self { id, kind: PhantomData }
    }

    /// The actor's ID.
    pub const fn id(self) -> ActorId {
        self.id
    }
}

impl<T> Copy for Ref<T> {}

impl<T> Clone for Ref<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for Ref<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Ref<T> {}

impl<T> Hash for Ref<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Debug for Ref<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ref({})", self.id)
    }
}

/// Typed sending from inside an actor.
pub trait Tell {
    /// Sends `msg` to `to` after `delay`.
    fn tell<T: Into<SimMsg>>(&mut self, to: Ref<T>, delay: SimDuration, msg: T);
}

impl Tell for Ctx<'_, SimMsg, Event> {
    fn tell<T: Into<SimMsg>>(&mut self, to: Ref<T>, delay: SimDuration, msg: T) {
        self.send_after(to.id(), delay, msg.into());
    }
}
