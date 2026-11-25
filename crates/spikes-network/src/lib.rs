//! Spiking networks on the `tempo` runtime.
//!
//! - [`messages`]: the message enum [`SimMsg`], events [`Event`], and typed handles ([`NeuronRef`], [`Tell`]).
//! - [`neuron`]: the neuron actor, a thin adapter between messages and `spikes-model`.
//! - [`inputs`]: Poisson and periodic input actors.
//! - [`modulators`]: neuromodulator releases and concentrations.
//! - [`build`]: builds a runtime network from a parsed `.boo` description.
//!
//! ```no_run
//! use spikes_network::{BuildOptions, Runtime, build};
//! use tempo_runtime::{RuntimeConfig, SimDuration, SimTime};
//!
//! let description = spikes_dna::parse(&std::fs::read_to_string("ring.boo").unwrap()).unwrap();
//! let mut rt = Runtime::new(RuntimeConfig::default());
//! let network = build(&mut rt, &description, &BuildOptions::default());
//! # let _ = network;
//! rt.run_until(SimTime::from_start(SimDuration::from_secs(1)));
//! ```

pub mod build;
pub mod inputs;
pub mod messages;
pub mod modulators;
pub mod neuron;
pub mod snapshot;

pub use build::{BuildOptions, Network, NewConnection, build};
pub use inputs::{PeriodicInput, PoissonInput};
pub use messages::{
    Event, InputMsg, InputRef, ModulatorMsg, ModulatorRef, NeuronMsg, NeuronRef, Ref, ReplayMsg, SimMsg, Tell,
};
pub use modulators::{ModulatorField, ModulatorKind, ModulatorRelease, ReleaseEvent};
pub use neuron::{NetworkParams, NeuronActor};
pub use snapshot::snapshot_codec;

/// A runtime for spiking networks.
pub type Runtime = tempo_runtime::Runtime<SimMsg, Event>;
