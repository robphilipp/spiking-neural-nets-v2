//! Behaviour of each neuron class, checked against hand-computed values.

use rand_core::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_model::{
    ExponentialStdp, Input, LearningRule, Model, NeuronKind, NeuronParams, NeuronState, Phase, Release, Resonance,
    Sign, Synapse, SynapseParams, Transition, Wakeup, WeightChange,
};
use tempo_core::{SimDuration, SimTime};

fn ms(v: f64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis_f64(v))
}

/// A neuron plus everything needed to drive it.
struct Rig {
    params: NeuronParams,
    synapse_params: Vec<SynapseParams>,
    state: NeuronState,
    synapses: Vec<Synapse>,
    rng: Xoshiro256PlusPlus,
    changes: Vec<WeightChange>,
}

impl Rig {
    fn new(params: NeuronParams) -> Self {
        Self {
            state: NeuronState::at_rest(&params),
            params,
            synapse_params: vec![SynapseParams::fixed()],
            synapses: vec![Synapse::new(1.0, 1.0, Sign::Excitatory, 0)],
            rng: Xoshiro256PlusPlus::seed_from_u64(1),
            changes: Vec::new(),
        }
    }

    fn input(&mut self, at_ms: f64, input: Input) -> spikes_model::Outcome {
        let model = Model::new(&self.params, &self.synapse_params);
        model.on_input(
            &mut self.state,
            &mut self.synapses,
            ms(at_ms),
            input,
            &mut self.rng,
            &mut self.changes,
        )
    }

    fn external(&mut self, at_ms: f64, mv: f64) -> spikes_model::Outcome {
        self.input(at_ms, Input::External { mv })
    }

    fn wakeup(&mut self, at: SimTime) -> spikes_model::Outcome {
        let model = Model::new(&self.params, &self.synapse_params);
        model.on_wakeup(
            &mut self.state,
            &mut self.synapses,
            at,
            &mut self.rng,
            &mut self.changes,
        )
    }
}

// ---------- monostable integrator ----------

#[test]
fn integrator_fires_when_inputs_arrive_close_enough_together() {
    // Threshold 10 mV, decay τ = 250 ms: two 6 mV inputs fire if 6 + 6·e^(−Δ/250) ≥ 10, i.e. Δ ≤ 250·ln 1.5 ≈ 101.4 ms.
    let mut close = Rig::new(NeuronParams::monostable_integrator(10.0));
    assert!(close.external(0.0, 6.0).spike.is_none());
    assert!(close.external(100.0, 6.0).spike.is_some());

    let mut far = Rig::new(NeuronParams::monostable_integrator(10.0));
    far.external(0.0, 6.0);
    let out = far.external(103.0, 6.0);
    assert!(out.spike.is_none());
    assert!((out.membrane_mv - (6.0 + 6.0 * (-103.0f64 / 250.0).exp())).abs() < 1e-9);
}

#[test]
fn rise_delay_follows_the_linear_approximation() {
    // From 0 to above threshold: delay = τrise·(θ − 0)/θ = 2 ms. From 8 mV: 2·(10 − 8)/10 = 0.4 ms.
    let mut a = Rig::new(NeuronParams::monostable_integrator(10.0));
    assert_eq!(a.external(0.0, 12.0).spike, Some(SimDuration::from_millis(2)));
    let mut b = Rig::new(NeuronParams {
        decay_ms: f64::INFINITY,
        ..NeuronParams::monostable_integrator(10.0)
    });
    b.external(0.0, 8.0);
    assert_eq!(b.external(1.0, 4.0).spike, Some(SimDuration::from_micros(400)));
}

#[test]
fn refractoriness_blocks_then_fades_without_accumulating() {
    let mut rig = Rig::new(NeuronParams {
        base_refractoriness: 50.0, // η(s) = −50/(s − 20) mV, s in ms
        ..NeuronParams::monostable_integrator(10.0)
    });
    assert!(rig.external(0.0, 12.0).spike.is_some());
    // Absolute refractory period (20 ms): even a huge input is lost.
    assert!(rig.external(10.0, 100.0).spike.is_none());
    // At s = 25 ms: η = −10 mV, so 15 mV gives an effective 5 mV.
    let out = rig.external(25.0, 11.0);
    assert!(out.spike.is_none());
    assert!((out.membrane_mv - 1.0).abs() < 1e-9);
    // Many small inputs don't pile up extra refractoriness (v1 subtracted η at every input).
    for i in 0..20 {
        rig.external(26.0 + i as f64, 0.01);
    }
    // At s = 70 ms: η = −1 mV. The stored potential is the decayed sum of the inputs, untouched by η.
    let out = rig.external(70.0, 0.0);
    let stored = rig.state.u;
    assert!((out.membrane_mv - (stored - 1.0)).abs() < 1e-9);
    assert_eq!(rig.state.spikes, 1, "stayed below threshold");
    assert!(
        stored > 11.0 * (-45.0f64 / 250.0).exp(),
        "decayed only by the membrane time constant: {stored}"
    );
}

// ---------- STDP ----------

fn learning_rig() -> Rig {
    let mut rig = Rig::new(NeuronParams {
        refractory_ms: 2.0,
        ..NeuronParams::monostable_integrator(10.0)
    });
    rig.synapse_params = vec![SynapseParams {
        learning: LearningRule::HardLimit(ExponentialStdp {
            inhibition_amplitude: 0.035,
            inhibition_period_ms: 10.0,
            excitation_amplitude: 0.015,
            excitation_period_ms: 10.0,
        }),
        limit: spikes_model::WeightLimit::Bounded { lower: 0.0, upper: 5.0 },
        ..SynapseParams::fixed()
    }];
    rig
}

#[test]
fn pre_before_post_potentiates_and_post_before_pre_depresses() {
    let mut rig = learning_rig();
    // A synaptic signal at 0 ms, then the neuron fires (from external input) at 5 ms.
    rig.input(0.0, Input::Synaptic { synapse: 0, mv: 1.0 });
    assert!(rig.external(5.0, 20.0).spike.is_some());
    let up = rig.changes.pop().expect("potentiation");
    assert!((f64::from(up.after - up.before) - 0.015 * (-0.5f64).exp()).abs() < 1e-6);

    // A synaptic signal 3 ms after that spike, not causing another: depression.
    rig.input(8.0, Input::Synaptic { synapse: 0, mv: 1.0 });
    let down = rig.changes.pop().expect("depression");
    assert!((f64::from(down.after - down.before) + 0.035 * (-0.3f64).exp()).abs() < 1e-6);
}

#[test]
fn potentiation_sums_every_signal_since_the_last_spike() {
    let mut rig = learning_rig();
    for t in [0.0, 2.0, 4.0] {
        rig.input(t, Input::Synaptic { synapse: 0, mv: 1.0 });
    }
    rig.external(10.0, 20.0);
    let expected: f64 = [10.0, 8.0, 6.0].iter().map(|d: &f64| 0.015 * (-d / 10.0).exp()).sum();
    let change = rig.changes.pop().unwrap();
    assert!((f64::from(change.after - change.before) - expected).abs() < 1e-6);
}

#[test]
fn inhibitory_synapses_lower_the_potential() {
    let mut rig = Rig::new(NeuronParams::monostable_integrator(10.0));
    rig.synapses = vec![
        Synapse::new(2.0, 2.0, Sign::Excitatory, 0),
        Synapse::new(1.5, 1.5, Sign::Inhibitory, 0),
    ];
    rig.input(0.0, Input::Synaptic { synapse: 0, mv: 3.0 }); // +6 mV
    let out = rig.input(0.0, Input::Synaptic { synapse: 1, mv: 3.0 }); // −4.5 mV
    assert!((out.membrane_mv - 1.5).abs() < 1e-9);
}

// ---------- stochastic release ----------

#[test]
fn release_is_stochastic_at_the_documented_resting_probability() {
    let release = Release {
        facilitation_base: 0.5,
        facilitation_magnitude: 0.0,
        facilitation_tau_ms: 10.0,
        depletion_base: 1.0,
        depletion_magnitude: 0.0,
        depletion_tau_ms: 10.0,
    };
    let mut rig = Rig::new(NeuronParams::monostable_integrator(1e9));
    rig.synapse_params = vec![SynapseParams {
        release: Some(release),
        ..SynapseParams::fixed()
    }];
    let n = 20_000;
    let released = (0..n)
        .filter(|i| {
            rig.input(*i as f64 * 1_000.0, Input::Synaptic { synapse: 0, mv: 1.0 })
                .released
        })
        .count();
    let p = 1.0 - (-0.5f64).exp();
    let observed = released as f64 / n as f64;
    assert!((observed - p).abs() < 0.015, "observed {observed}, expected {p}");
}

// ---------- bistable integrator ----------

fn bistable() -> NeuronParams {
    NeuronParams {
        kind: NeuronKind::BistableIntegrator {
            resting_threshold_mv: 10.0,
            tonic_rate_hz: 10.0,
        },
        ..NeuronParams::monostable_integrator(15.0)
    }
}

/// Follows wake-ups until the neuron stops asking for them; returns the tonic spike times in ms.
fn run_tonic(rig: &mut Rig, mut next: Wakeup, mut now: SimTime, inputs: &[(f64, f64)]) -> Vec<f64> {
    let mut spikes = Vec::new();
    let mut inputs = inputs.to_vec();
    while let Wakeup::In(delay) = next {
        let at = now + delay;
        // Deliver any input that comes first.
        if let Some(&(t, mv)) = inputs.first()
            && ms(t) < at
        {
            inputs.remove(0);
            let out = rig.external(t, mv);
            if out.wakeup != Wakeup::Keep {
                next = out.wakeup;
                now = ms(t);
            }
            continue;
        }
        let out = rig.wakeup(at);
        if out.spike.is_some() {
            spikes.push(at.since_start().as_millis_f64());
        }
        next = out.wakeup;
        now = at;
    }
    spikes
}

#[test]
fn bistable_integrator_fires_tonically_until_the_potential_decays() {
    let mut rig = Rig::new(bistable());
    let start = rig.external(0.0, 20.0);
    assert_eq!(start.transition, Some(Transition::StartedTonic));
    assert!(start.spike.is_some());
    assert_eq!(start.membrane_mv, 15.0, "capped at the firing threshold");
    // u = 15·e^(−t/250): still ≥ 10 at 100 ms (10.05), below at 200 ms (6.7). Tonic spikes at 100 and 200 ms.
    let spikes = run_tonic(&mut rig, start.wakeup, ms(0.0), &[]);
    assert_eq!(spikes, [100.0, 200.0]);
    assert_eq!(rig.state.phase, Phase::Rest);
}

#[test]
fn bistable_integrator_keeps_integrating_input_while_tonic() {
    let mut rig = Rig::new(bistable());
    let start = rig.external(0.0, 20.0);
    // An input at 150 ms tops the potential back up, so firing continues past 200 ms (v1 dropped such input).
    let spikes = run_tonic(&mut rig, start.wakeup, ms(0.0), &[(150.0, 10.0)]);
    assert!(spikes.len() > 2, "{spikes:?}");
    assert_eq!(&spikes[..2], [100.0, 200.0]);
}

// ---------- resonators ----------

fn resonator(threshold: f64, damping_ms: f64) -> NeuronParams {
    NeuronParams {
        kind: NeuronKind::MonostableResonator(Resonance {
            frequency_hz: 50.0, // period 20 ms
            damping_ms,
        }),
        ..NeuronParams::monostable_integrator(threshold)
    }
}

#[test]
fn a_resonator_fires_for_inputs_in_phase_but_not_out_of_phase() {
    // One period later the first pulse has decayed to 6·e^(−20/50) = 4.02 mV, in phase: 4.02 + 6 ≥ 10.
    let mut in_phase = Rig::new(resonator(10.0, 50.0));
    in_phase.external(0.0, 6.0);
    assert!(in_phase.external(20.0, 6.0).spike.is_some());
    // Half a period later the oscillation is at −6·e^(−10/50) = −4.9 mV: the second pulse cancels it.
    let mut out_of_phase = Rig::new(resonator(10.0, 50.0));
    out_of_phase.external(0.0, 6.0);
    let out = out_of_phase.external(10.0, 6.0);
    assert!(out.spike.is_none());
    assert!((out.membrane_mv - (6.0 - 6.0 * (-0.2f64).exp())).abs() < 1e-9);
    assert_eq!(out.wakeup, Wakeup::Cancel, "too small to reach the threshold later");
}

#[test]
fn a_resonator_predicts_and_fires_at_a_later_threshold_crossing() {
    // 11 mV at 0 ms; three-quarters of a period later the oscillation sits at y = 0, x = +10.67 mV, so a 6 mV
    // pulse leaves it rising toward an amplitude of about 12.2 mV, above the 12 mV threshold.
    let mut rig = Rig::new(resonator(12.0, 500.0));
    rig.external(0.0, 11.0);
    let out = rig.external(15.0, 6.0);
    assert!(out.spike.is_none());
    let Wakeup::In(delay) = out.wakeup else {
        panic!("expected a predicted crossing, got {:?}", out.wakeup);
    };
    assert!(delay < SimDuration::from_millis(5), "within a quarter period: {delay}");
    // Just before the predicted time it's below threshold; at the predicted time it fires.
    let mut early = Rig::new(resonator(12.0, 500.0));
    early.external(0.0, 11.0);
    early.external(15.0, 6.0);
    let before = early.wakeup(ms(15.0) + delay - SimDuration::from_micros(1));
    assert!(before.spike.is_none());
    assert!(rig.wakeup(ms(15.0) + delay).spike.is_some());
}

fn bistable_resonator() -> NeuronParams {
    NeuronParams {
        kind: NeuronKind::BistableResonator {
            resonance: Resonance {
                frequency_hz: 50.0,
                damping_ms: 50.0,
            },
            cycle_mv: 12.0,
            separatrix_mv: 6.0,
        },
        ..NeuronParams::monostable_integrator(10.0)
    }
}

#[test]
fn a_bistable_resonator_fires_at_its_resonant_frequency() {
    let mut rig = Rig::new(bistable_resonator());
    let start = rig.external(0.0, 12.0);
    assert_eq!(start.transition, Some(Transition::StartedTonic));
    assert_eq!(start.wakeup, Wakeup::In(SimDuration::from_millis(20)));
    for k in 1..=5 {
        let out = rig.wakeup(ms(20.0 * k as f64));
        assert!(out.spike.is_some());
        assert_eq!(out.wakeup, Wakeup::In(SimDuration::from_millis(20)));
    }
    assert_eq!(rig.state.spikes, 6);
}

#[test]
fn only_a_well_timed_pulse_switches_a_bistable_resonator_off() {
    // At the peak of the cycle (y = 12) an −8 mV pulse leaves radius 4 < 6: back to rest.
    let mut at_peak = Rig::new(bistable_resonator());
    at_peak.external(0.0, 12.0);
    at_peak.wakeup(ms(20.0));
    let off = at_peak.external(20.0, -8.0);
    assert_eq!(off.transition, Some(Transition::Rested));
    assert_eq!(at_peak.state.phase, Phase::Rest);

    // Half a cycle later (y = −12) the same pulse leaves radius 20: it stays on the cycle, phase shifted.
    let mut at_trough = Rig::new(bistable_resonator());
    at_trough.external(0.0, 12.0);
    at_trough.wakeup(ms(20.0));
    let on = at_trough.external(30.0, -8.0);
    assert_eq!(on.transition, None);
    assert_eq!(at_trough.state.phase, Phase::Tonic);
    assert!(matches!(on.wakeup, Wakeup::In(_)));
}

// ---------- noise and determinism ----------

#[test]
fn noise_is_reproducible_and_absent_when_zero() {
    let noisy = NeuronParams {
        noise_mv: 0.5,
        ..NeuronParams::monostable_integrator(100.0)
    };
    let run = |params: &NeuronParams, seed| {
        let mut rig = Rig::new(params.clone());
        rig.rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        (0..50)
            .map(|i| rig.external(i as f64 * 5.0, 1.0).membrane_mv)
            .collect::<Vec<_>>()
    };
    assert_eq!(run(&noisy, 1), run(&noisy, 1));
    assert_ne!(run(&noisy, 1), run(&noisy, 2));
    let quiet = NeuronParams::monostable_integrator(100.0);
    assert_eq!(
        run(&quiet, 1),
        run(&quiet, 2),
        "no noise: the random stream doesn't matter"
    );
}

#[test]
fn noise_never_takes_away_input_arriving_at_rest() {
    // A quiet neuron sits at its floor (the resting potential): noise gathered while quiet can raise it but not
    // push it below, so it never cancels input. (It used to: the floor was applied after adding the input, and
    // after 250 quiet ms the noise is ~8 mV either way here; a ring network lost spikes to it and stopped.)
    let noisy = NeuronParams {
        noise_mv: 0.5,
        ..NeuronParams::monostable_integrator(100.0)
    };
    let mut lowest = f64::MAX;
    for seed in 0..200 {
        let mut rig = Rig::new(noisy.clone());
        rig.rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        lowest = lowest.min(rig.external(250.0, 3.0).membrane_mv);
    }
    assert!(lowest >= 3.0 - 1e-9, "input was lost to noise: {lowest} mV");
}
