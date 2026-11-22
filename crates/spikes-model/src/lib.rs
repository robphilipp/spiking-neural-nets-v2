//! The spiking neuron model as plain Rust: no actors, no threads.
//!
//! Every equation is a function over plain data (plus a random-number generator where the model is stochastic),
//! so each can be tested on its own, and the actor layer (`spikes-network`) only has to turn results into
//! messages. Potentials are in mV, times in ms (see [`units`]).
//!
//! - [`neuron`]: monostable and bistable integrators and resonators; [`neuron::Model::on_input`] and
//!   [`neuron::Model::on_wakeup`] are the per-event updates.
//! - [`synapse`]: weights (decay, limits, stickiness, noise), stochastic release, STDP traces.
//! - [`stdp`]: the learning rules.
//! - [`intrinsic`]: intrinsic plasticity, with optional homeostasis.
//! - [`modulation`]: neuromodulator receptors and their effects.
//! - [`crossing`]: predicting threshold crossings between events.
//!
//! The model follows the documented v1 model (wiki, `dna-defs.md`, paper), not v1's code where the two differ;
//! the differences are listed in `docs/plans/neurons/README.md`.

pub mod crossing;
pub mod intrinsic;
pub mod modulation;
pub mod neuron;
pub mod stdp;
pub mod synapse;
pub mod units;

pub use intrinsic::{Homeostasis, IntrinsicParams};
pub use modulation::{Modulators, NoModulators, Pulse, Receptor, ThreeFactor};
pub use neuron::{
    Input, Model, NeuronKind, NeuronParams, NeuronState, Outcome, Phase, Resonance, Transition, Wakeup, WeightChange,
};
pub use stdp::{AlphaStdp, ExponentialStdp, LearningRule};
pub use synapse::{Release, Sign, Stickiness, Synapse, SynapseParams, WeightDecay, WeightLimit};
