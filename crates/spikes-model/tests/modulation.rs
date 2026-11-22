//! Neuromodulation effects on single neurons.

use rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_model::modulation::{pulse_integral, pulse_level};
use spikes_model::{
    ExponentialStdp, Input, LearningRule, Model, Modulators, NeuronParams, NeuronState, Pulse, Receptor, Sign, Synapse,
    SynapseParams, ThreeFactor, WeightChange,
};
use tempo_core::{SimDuration, SimTime};

fn ms(v: f64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis_f64(v))
}

/// One modulator (index 0) given as pulses.
struct Pulses {
    pulses: Vec<Pulse>,
    clearance_ms: f64,
}

impl Modulators for Pulses {
    fn level(&self, _: u16, t: SimTime) -> f64 {
        pulse_level(self.pulses.iter().copied(), self.clearance_ms, t)
    }
    fn decayed_integral(&self, _: u16, t0: SimTime, t1: SimTime, tau_ms: f64) -> f64 {
        pulse_integral(self.pulses.iter().copied(), self.clearance_ms, t0, t1, tau_ms)
    }
    fn clearance_ms(&self, _: u16) -> f64 {
        self.clearance_ms
    }
}

/// A unit pulse of modulator arriving at `at_ms`, cleared with τ = 100 ms.
fn pulse_at(at_ms: f64) -> Pulses {
    Pulses {
        pulses: vec![Pulse {
            arrival: ms(at_ms),
            amplitude: 1.0,
        }],
        clearance_ms: 100.0,
    }
}

struct Rig {
    params: NeuronParams,
    synapse_params: Vec<SynapseParams>,
    receptors: Vec<Receptor>,
    modulators: Pulses,
    state: NeuronState,
    synapses: Vec<Synapse>,
    rng: Xoshiro256PlusPlus,
    changes: Vec<WeightChange>,
}

impl Rig {
    fn new(receptor: Receptor, modulators: Pulses) -> Self {
        let params = NeuronParams {
            decay_ms: f64::INFINITY,
            ..NeuronParams::monostable_integrator(10.0)
        };
        Self {
            state: NeuronState::at_rest(&params),
            params,
            synapse_params: vec![SynapseParams::fixed()],
            receptors: vec![receptor],
            modulators,
            synapses: vec![Synapse::new(1.0, 1.0, Sign::Excitatory, 0)],
            rng: Xoshiro256PlusPlus::seed_from_u64(1),
            changes: Vec::new(),
        }
    }

    fn modulation(&mut self, at_ms: f64) -> spikes_model::Outcome {
        let model = Model::new(&self.params, &self.synapse_params).with_modulation(&self.receptors, &self.modulators);
        model.on_modulation(
            &mut self.state,
            &mut self.synapses,
            ms(at_ms),
            &mut self.rng,
            &mut self.changes,
        )
    }

    fn input(&mut self, at_ms: f64, input: Input) -> spikes_model::Outcome {
        let model = Model::new(&self.params, &self.synapse_params).with_modulation(&self.receptors, &self.modulators);
        model.on_input(
            &mut self.state,
            &mut self.synapses,
            ms(at_ms),
            input,
            &mut self.rng,
            &mut self.changes,
        )
    }

    fn synaptic(&mut self, at_ms: f64, mv: f64) -> spikes_model::Outcome {
        self.input(at_ms, Input::Synaptic { synapse: 0, mv })
    }
}

#[test]
fn excitatory_gain_scales_synaptic_input() {
    // α_exc = 1 and c = 1: synaptic input doubles. External input is unaffected.
    let mut rig = Rig::new(
        Receptor {
            excitatory_gain: 1.0,
            ..Receptor::new(0)
        },
        pulse_at(0.0),
    );
    assert_eq!(rig.synaptic(0.0, 3.0).membrane_mv, 6.0);
    assert_eq!(rig.input(0.0, Input::External { mv: 1.0 }).membrane_mv, 7.0);
    // Before the modulator arrives there's no effect.
    let mut early = Rig::new(
        Receptor {
            excitatory_gain: 1.0,
            ..Receptor::new(0)
        },
        pulse_at(50.0),
    );
    assert_eq!(early.synaptic(10.0, 3.0).membrane_mv, 3.0);
}

#[test]
fn inhibitory_gain_can_silence_inhibition() {
    let mut rig = Rig::new(
        Receptor {
            inhibitory_gain: -1.0,
            ..Receptor::new(0)
        },
        pulse_at(0.0),
    );
    rig.synapses = vec![Synapse::new(2.0, 2.0, Sign::Inhibitory, 0)];
    rig.input(0.0, Input::External { mv: 5.0 });
    assert_eq!(
        rig.synaptic(0.0, 1.0).membrane_mv,
        5.0,
        "inhibition fully suppressed at c = 1"
    );
}

#[test]
fn threshold_modulation_lets_weaker_input_fire() {
    // α_θ = −0.5 at c = 1 halves the 10 mV threshold.
    let receptor = Receptor {
        threshold: -0.5,
        ..Receptor::new(0)
    };
    assert!(Rig::new(receptor, pulse_at(0.0)).synaptic(1.0, 6.0).spike.is_some());
    assert!(Rig::new(receptor, pulse_at(50.0)).synaptic(1.0, 6.0).spike.is_none());
}

#[test]
fn direct_potential_fires_a_resting_neuron_when_the_modulator_arrives() {
    let mut rig = Rig::new(
        Receptor {
            direct_mv: 4.0,
            ..Receptor::new(0)
        },
        pulse_at(20.0),
    );
    rig.input(0.0, Input::External { mv: 7.0 }); // 7 mV, below the 10 mV threshold
    assert!(rig.modulation(10.0).spike.is_none(), "nothing has arrived yet");
    let at_arrival = rig.modulation(20.0);
    assert!(at_arrival.spike.is_some(), "7 mV + 4 mV·c(=1) ≥ 10 mV");
}

fn stdp() -> SynapseParams {
    SynapseParams {
        learning: LearningRule::HardLimit(ExponentialStdp {
            inhibition_amplitude: 0.0,
            inhibition_period_ms: 10.0,
            excitation_amplitude: 0.02,
            excitation_period_ms: 10.0,
        }),
        ..SynapseParams::fixed()
    }
}

#[test]
fn plasticity_modulation_scales_stdp() {
    let mut rig = Rig::new(
        Receptor {
            plasticity: 2.0,
            ..Receptor::new(0)
        },
        pulse_at(0.0),
    );
    rig.synapse_params = vec![stdp()];
    rig.synaptic(1.0, 1.0);
    rig.input(1.0, Input::External { mv: 20.0 }); // fires with the synaptic signal 0 ms before
    let change = rig.changes.pop().unwrap();
    // Δw = 0.02·e⁰, times (1 + 2·c) with c = e^(−1/100) at 1 ms.
    let c = (-0.01f64).exp();
    assert!((f64::from(change.after - change.before) - 0.02 * (1.0 + 2.0 * c)).abs() < 1e-6);
}

#[test]
fn three_factor_learning_waits_for_the_modulator() {
    let receptor = Receptor {
        three_factor: Some(ThreeFactor {
            eligibility_ms: 500.0,
            rate: 2.0,
        }),
        ..Receptor::new(0)
    };
    // Pre-before-post pairing at 1 ms marks the synapse; dopamine arrives at 200 ms; the synapse's next signal
    // (at 400 ms) brings its weight up to date.
    let run = |modulators: Pulses| {
        let mut rig = Rig::new(receptor, modulators);
        rig.synapse_params = vec![stdp()];
        rig.synaptic(1.0, 1.0);
        rig.input(1.0, Input::External { mv: 20.0 });
        assert!(
            rig.changes.is_empty(),
            "STDP goes to the eligibility trace, not the weight"
        );
        assert!((f64::from(rig.synapses[0].eligibility) - 0.02).abs() < 1e-7);
        rig.synaptic(400.0, 0.0);
        f64::from(rig.synapses[0].weight) - 1.0
    };
    assert_eq!(
        run(Pulses {
            pulses: Vec::new(),
            clearance_ms: 100.0
        }),
        0.0,
        "no dopamine, no learning"
    );

    let dopamine = pulse_at(200.0);
    let integral = pulse_integral(dopamine.pulses.clone(), 100.0, ms(1.0), ms(400.0), 500.0);
    let expected = 2.0 * 0.02 * integral / 1e3;
    let got = run(dopamine);
    assert!(got > 0.0);
    assert!((got - expected).abs() < 1e-6, "{got} vs {expected}");
}
