//! Saving a running network (see `tempo_runtime::Snapshot`): what changes in each kind of actor, and in the
//! modulator field. Everything else (parameters, positions, routes' owners) is rebuilt from the description.

use serde::{Deserialize, Serialize};
use spikes_model::{NeuronState, Synapse};
use tempo_runtime::{ActorId, Persist, PersistField, RouteId, SnapshotCodec, Timer};

use crate::messages::{Event, SimMsg};
use crate::modulators::{ModulatorField, ModulatorRelease, ReleaseEvent};
use crate::neuron::NeuronActor;
use crate::{PeriodicInput, PoissonInput, PulseInput, SignalInput};

/// A neuron, as saved.
#[derive(Debug, Serialize, Deserialize)]
pub struct NeuronSnapshot {
    state: NeuronState,
    synapses: Vec<Synapse>,
    pre: Vec<ActorId>,
    out: Option<RouteId>,
    timer: Timer,
}

impl Persist<SimMsg, Event> for NeuronActor {
    type State = NeuronSnapshot;

    fn save(&self) -> NeuronSnapshot {
        NeuronSnapshot {
            state: self.state.clone(),
            synapses: self.synapses.clone(),
            pre: self.pre.clone(),
            out: self.out,
            timer: self.timer.clone(),
        }
    }

    fn restore(&mut self, s: NeuronSnapshot) {
        self.state = s.state;
        self.synapses = s.synapses;
        self.pre = s.pre;
        self.out = s.out;
        self.timer = s.timer;
    }
}

/// A signal source, as saved: its encoders and its next sample (its formula and targets are rebuilt from the
/// input file).
#[derive(Debug, Serialize, Deserialize)]
pub struct SignalSnapshot {
    encoders: Vec<spikes_signal::Channel>,
    next: u64,
}

impl Persist<SimMsg, Event> for SignalInput {
    type State = SignalSnapshot;

    fn save(&self) -> SignalSnapshot {
        SignalSnapshot {
            encoders: self.encoders.clone(),
            next: self.next,
        }
    }

    fn restore(&mut self, s: SignalSnapshot) {
        self.encoders = s.encoders;
        self.next = s.next;
    }
}

impl PersistField for ModulatorField {
    type State = Vec<ReleaseEvent>;

    fn save(&self) -> Vec<ReleaseEvent> {
        self.releases().to_vec()
    }

    fn restore(&mut self, releases: Vec<ReleaseEvent>) {
        self.set_releases(releases);
    }
}

/// How a network's actors and fields are saved. (Inputs and modulator release points keep nothing of their own:
/// their timing is in their pending events and random streams, which the runtime saves.)
pub fn snapshot_codec() -> SnapshotCodec<SimMsg, Event> {
    SnapshotCodec::new()
        .actor::<NeuronActor>("neuron")
        .stateless::<PoissonInput>("poisson input")
        .stateless::<PeriodicInput>("periodic input")
        .stateless::<PulseInput>("pulse input")
        .actor::<SignalInput>("signal input")
        .stateless::<ModulatorRelease>("modulator release")
        .field::<ModulatorField>()
}
