//! The neuron actor: turns messages into model updates, and model outcomes into sends, timers and events.

use std::sync::Arc;

use spikes_model::{Input, Model, NeuronParams, NeuronState, Outcome, Synapse, SynapseParams, Wakeup, WeightChange};
use tempo_runtime::{Actor, ActorId, Ctx, EdgeIdx, RouteId, Timer};

use crate::messages::{Event, NeuronMsg, SimMsg};

/// Parameters shared by every neuron in a network: population and synapse tables, and recording options.
#[derive(Debug)]
pub struct NetworkParams {
    /// Neuron parameter sets, indexed by [`NeuronActor::population`].
    pub neurons: Vec<NeuronParams>,
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
    timer: Timer,
    changes: Vec<WeightChange>,
}

impl NeuronActor {
    /// A neuron at rest, without synapses or outgoing connections yet.
    pub fn new(params: Arc<NetworkParams>, population: u32) -> Self {
        let state = NeuronState::at_rest(&params.neurons[population as usize]);
        Self {
            state,
            synapses: Vec::new(),
            pre: Vec::new(),
            params,
            population,
            out: None,
            timer: Timer::default(),
            changes: Vec::new(),
        }
    }

    fn input(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, input: Input) {
        let model = Model {
            neuron: &self.params.neurons[self.population as usize],
            synapses: &self.params.synapses,
        };
        let outcome = model.on_input(
            &mut self.state,
            &mut self.synapses,
            ctx.now(),
            input,
            ctx.rng(),
            &mut self.changes,
        );
        self.handle(ctx, outcome);
    }

    fn wake(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>) {
        let model = Model {
            neuron: &self.params.neurons[self.population as usize],
            synapses: &self.params.synapses,
        };
        let outcome = model.on_wakeup(
            &mut self.state,
            &mut self.synapses,
            ctx.now(),
            ctx.rng(),
            &mut self.changes,
        );
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
            SimMsg::Neuron(NeuronMsg::External { mv }) => self.input(ctx, Input::External { mv: f64::from(mv) }),
            SimMsg::Neuron(NeuronMsg::Wake(token)) => {
                if self.timer.fire(token) {
                    self.wake(ctx);
                }
            }
            // A spike sent directly rather than along a route has no synapse: treat it as external input.
            SimMsg::Neuron(NeuronMsg::Spike { mv }) => self.input(ctx, Input::External { mv: f64::from(mv) }),
            SimMsg::Input(_) => {}
        }
    }

    fn on_route_message(&mut self, ctx: &mut Ctx<'_, SimMsg, Event>, _from: ActorId, edge: EdgeIdx, msg: SimMsg) {
        if let SimMsg::Neuron(NeuronMsg::Spike { mv }) = msg {
            self.input(
                ctx,
                Input::Synaptic {
                    synapse: edge.0,
                    mv: f64::from(mv),
                },
            );
        }
    }
}
