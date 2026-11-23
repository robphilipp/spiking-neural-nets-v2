//! The neuron actor: turns messages into model updates, and model outcomes into sends, timers and events.

use std::sync::Arc;

use spikes_model::{
    Input, Model, Modulators, NeuronParams, NeuronState, NoModulators, Outcome, Receptor, Synapse, SynapseParams,
    Wakeup, WeightChange,
};
use tempo_runtime::{Actor, ActorId, Ctx, EdgeIdx, RouteId, Timer};

use crate::messages::{Event, NeuronMsg, SimMsg};
use crate::modulators::{ModulatorField, ModulatorKind, release};

/// Parameters shared by every neuron in a network: population and synapse tables, and recording options.
#[derive(Debug)]
pub struct NetworkParams {
    /// Neuron parameter sets, indexed by [`NeuronActor::population`].
    pub neurons: Vec<NeuronParams>,
    /// Each population's neuromodulator receptors.
    pub receptors: Vec<Vec<Receptor>>,
    /// Each population's modulator release on spiking: (modulator, amount).
    pub releases: Vec<Option<(u16, f64)>>,
    /// The network's modulators.
    pub modulators: Arc<Vec<ModulatorKind>>,
    /// Synapse parameter sets, indexed by [`Synapse::params`].
    pub synapses: Vec<SynapseParams>,
    /// Emit an [`Event::Weight`] for every weight change from learning.
    pub record_weights: bool,
}

/// A neuron.
#[derive(Debug)]
pub struct NeuronActor {
    /// Dynamic state.
    pub state: NeuronState,
    /// Incoming synapses; a route edge's index is its position here.
    pub synapses: Vec<Synapse>,
    /// The sending neuron of each incoming synapse.
    pub pre: Vec<ActorId>,
    /// Shared parameter tables.
    pub params: Arc<NetworkParams>,
    /// This neuron's parameter set.
    pub population: u32,
    /// Outgoing connections.
    pub out: Option<RouteId>,
    /// Soma position in µm (where modulator concentrations are evaluated, and where releases happen).
    pub position: [f64; 3],
    timer: Timer,
    changes: Vec<WeightChange>,
}

impl NeuronActor {
    /// Adds an incoming synapse from `pre`; returns its index (the route edge's `EdgeIdx`).
    pub fn add_synapse(&mut self, pre: ActorId, synapse: Synapse) -> u32 {
        self.synapses.push(synapse);
        self.pre.push(pre);
        u32::try_from(self.synapses.len() - 1).expect("fewer than 2³² synapses")
    }

    /// Prunes the (first live) synapse from `pre`: it keeps its slot, so other synapses keep their indices, but
    /// has no sender and no weight. Returns its index.
    pub fn prune_synapse(&mut self, pre: ActorId) -> Option<u32> {
        let i = self.pre.iter().position(|&p| p == pre)?;
        self.pre[i] = ActorId::WORLD;
        let s = &mut self.synapses[i];
        s.weight = 0.0;
        s.equilibrium = 0.0;
        s.eligibility = 0.0;
        Some(i as u32)
    }

    /// A neuron at rest, without synapses or outgoing connections yet.
    pub fn new(params: Arc<NetworkParams>, population: u32, position: [f64; 3]) -> Self {
        let state = NeuronState::at_rest(&params.neurons[population as usize]);
        Self {
            state,
            synapses: Vec::new(),
            pre: Vec::new(),
            params,
            population,
            out: None,
            position,
            timer: Timer::default(),
            changes: Vec::new(),
        }
    }

    /// Runs one model update with this neuron's receptors and the shard's modulator concentrations.
    fn update(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, what: Update) {
        let now = ctx.now();
        let population = self.population as usize;
        let (field, rng) = ctx.field_and_rng::<ModulatorField>();
        let view = field.map(|f| f.at(self.position));
        let modulators: &dyn Modulators = match &view {
            Some(v) => v,
            None => &NoModulators,
        };
        let model = Model::new(&self.params.neurons[population], &self.params.synapses)
            .with_modulation(&self.params.receptors[population], modulators);
        let (state, synapses, changes) = (&mut self.state, &mut self.synapses, &mut self.changes);
        let outcome = match what {
            Update::Input(input) => model.on_input(state, synapses, now, input, rng, changes),
            Update::Wake => model.on_wakeup(state, synapses, now, rng, changes),
            Update::Modulate => model.on_modulation(state, synapses, now, rng, changes),
        };
        self.handle(ctx, outcome);
    }

    fn handle(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, outcome: Outcome) {
        if let Some(rise) = outcome.spike {
            let at = ctx.now() + rise;
            ctx.emit(Event::Spike { at });
            if let Some(route) = self.out {
                let mv = self.params.neurons[self.population as usize].spike_mv as f32;
                ctx.fanout_at(route, at, SimMsg::Neuron(NeuronMsg::Spike { mv }));
            }
            if let Some((kind, amount)) = self.params.releases[self.population as usize] {
                release(ctx, &self.params.modulators, kind, amount, self.position, at);
            }
        }
        if let Some(transition) = outcome.transition {
            ctx.emit(Event::Transition(transition));
        }
        match outcome.wakeup {
            Wakeup::Keep => {}
            Wakeup::Cancel => self.timer.cancel(),
            Wakeup::In(delay) => {
                let token = self.timer.arm();
                ctx.schedule_self(delay, SimMsg::Neuron(NeuronMsg::Wake(token)));
            }
        }
        if self.params.record_weights {
            for change in self.changes.drain(..) {
                ctx.emit(Event::Weight {
                    synapse: change.synapse,
                    pre: self.pre[change.synapse as usize],
                    before: change.before,
                    after: change.after,
                });
            }
        } else {
            self.changes.clear();
        }
        if ctx.is_observed() && outcome.released {
            ctx.emit(Event::Membrane {
                mv: outcome.membrane_mv as f32,
            });
        }
    }
}

impl Actor<SimMsg, Event> for NeuronActor {
    fn on_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, msg: SimMsg) {
        match msg {
            SimMsg::Neuron(NeuronMsg::External { mv }) => {
                self.update(ctx, Update::Input(Input::External { mv: f64::from(mv) }));
            }
            SimMsg::Neuron(NeuronMsg::Wake(token)) => {
                if self.timer.fire(token) {
                    self.update(ctx, Update::Wake);
                }
            }
            SimMsg::Neuron(NeuronMsg::Modulate) => self.update(ctx, Update::Modulate),
            // A spike sent directly rather than along a route has no synapse: treat it as external input.
            SimMsg::Neuron(NeuronMsg::Spike { mv }) => {
                self.update(ctx, Update::Input(Input::External { mv: f64::from(mv) }));
            }
            SimMsg::Input(_) | SimMsg::Modulator(_) => {}
        }
    }

    fn on_route_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, edge: EdgeIdx, msg: SimMsg) {
        if let SimMsg::Neuron(NeuronMsg::Spike { mv }) = msg {
            self.update(
                ctx,
                Update::Input(Input::Synaptic {
                    synapse: edge.0,
                    mv: f64::from(mv),
                }),
            );
        }
    }
}

/// What a model update is for.
#[derive(Copy, Clone, Debug)]
enum Update {
    Input(Input),
    Wake,
    Modulate,
}
