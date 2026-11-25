// line sensor network
// For parameters that accept units, if they are not specified, they default to:
// • distances to µm
// • times to ms
// • conductance speeds to m/s
// • electric potentials to mV
// • frequencies to Hz
// • magnetic flux to Wb
(
neuron_types=[
    (name=input, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.05 mV, spike_potential=0.5 mV, conduction_speed=0.1 m/s, intrinsic_plasticity=(step=0 mV, amplitude=0 mV, decay_time=3600 s)),
    (name=inhib, threshold=1 mV, inhibitory=true, refractory_period=0.1 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.05 mV, spike_potential=2 mV, conduction_speed=0.08 m/s, intrinsic_plasticity=(step=0 mV, amplitude=0 mV, decay_time=3600 s)),
    (name=output, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.05 mV, spike_potential=1 mV, conduction_speed=1 m/s, intrinsic_plasticity=(step=0 mV, amplitude=0.001 mV, decay_time=3600 s)),
],
synapse_types=[
    (name=stdp, limits=(min=0, max=1), release=(facilitation_base=1, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=10, depletion_step=1, depletion_time=100 ms), weight_noise=0.001, learning=stdp),
    (name=fixed, decay=(time=10 s), limits=(min=0, max=2), release=(facilitation_base=1000, facilitation_step=0, facilitation_time=100 ms, depletion_base=1000, depletion_step=0, depletion_time=100 ms), weight_noise=0.001),
    (name=fixed_2, limits=(min=0, max=1), release=(facilitation_base=1, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=10, depletion_step=1, depletion_time=100 ms), weight_noise=0.001),
],
neurons=[
    (name=input-1, type=input, position=[-300 µm, 0 µm, 200 µm]),
    (name=input-2, type=input, position=[300 µm, 0 µm, 200 µm]),
    (name=inhib-1, type=inhib, position=[-300 µm, 0 µm, 10 µm]),
    (name=inhib-2, type=inhib, position=[0 µm, 0 µm, 10 µm]),
    (name=inhib-3, type=inhib, position=[300 µm, 0 µm, 10 µm]),
    (name=output-1, type=output, position=[-300 µm, 0 µm, 0 µm]),
    (name=output-2, type=output, position=[0 µm, 0 µm, 0 µm]),
    (name=output-3, type=output, position=[300 µm, 0 µm, 0 µm]),
],
connections=[
    (from=input-1, to=output-1, type=stdp, weight=0.5),
    (from=input-2, to=output-1, type=stdp, weight=0.5),
    (from=input-1, to=output-2, type=stdp, weight=0.5),
    (from=input-2, to=output-2, type=stdp, weight=0.5),
    (from=input-1, to=output-3, type=stdp, weight=0.5),
    (from=input-2, to=output-3, type=stdp, weight=0.5),
    (from=output-1, to=inhib-1, type=fixed, weight=2),
    (from=output-2, to=[inhib-1, inhib-3], type=fixed, weight=2),
    (from=output-3, to=inhib-3, type=fixed, weight=2),
    (from=inhib-1, to=output-3, type=fixed_2, weight=1),
    (from=inhib-2, to=[output-1, output-3], type=fixed_2, weight=1),
    (from=inhib-3, to=output-1, type=fixed_2, weight=1),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.04, depression_time=50 ms, potentiation=0.02, potentiation_time=50 ms),
],
)
