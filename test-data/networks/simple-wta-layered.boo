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
    (name=neuron, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=0.5 mV, conduction_speed=0.1 m/s),
    (name=inhib, threshold=1 mV, inhibitory=true, refractory_period=0.1 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=2 mV, conduction_speed=0.08 m/s),
    (name=output, threshold=2 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=2 s, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=1 mV, conduction_speed=1 m/s),
],
synapse_types=[
    (name=stdp, limits=(min=0, max=1), release=(facilitation_base=1000, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=1000, depletion_step=0.1, depletion_time=100 ms), learning=stdp),
    (name=stdp_2, decay=(time=2 h), limits=(min=0, max=1), release=(facilitation_base=1, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=10, depletion_step=1, depletion_time=100 ms), learning=stdp),
    (name=fixed, decay=(time=2 h), limits=(min=0, max=1), release=(facilitation_base=1000, facilitation_step=0, facilitation_time=100 ms, depletion_base=1000, depletion_step=0, depletion_time=100 ms)),
    (name=fixed_2, decay=(time=2 h), limits=(min=0, max=1), release=(facilitation_base=1, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=10, depletion_step=1, depletion_time=100 ms)),
],
neurons=[
    (name=input-1, type=neuron, position=[-300 µm, 0 µm, 100 µm]),
    (name=input-2, type=neuron, position=[300 µm, 0 µm, 100 µm]),
    (name=hidden-1, type=neuron, position=[-300 µm, 0 µm, 50 µm]),
    (name=hidden-2, type=neuron, position=[0 µm, 0 µm, 50 µm]),
    (name=hidden-3, type=neuron, position=[300 µm, 0 µm, 50 µm]),
    (name=inhib-1, type=inhib, position=[-290 µm, 0 µm, 0 µm]),
    (name=inhib-2, type=inhib, position=[0 µm, 0 µm, 10 µm]),
    (name=inhib-3, type=inhib, position=[290 µm, 0 µm, 0 µm]),
    (name=output-1, type=output, position=[-300 µm, 0 µm, 0 µm]),
    (name=output-2, type=output, position=[0 µm, 0 µm, 0 µm]),
    (name=output-3, type=output, position=[300 µm, 0 µm, 0 µm]),
],
connections=[
    (from=input-1, to=hidden-1, type=stdp, weight=uniform(0.4, 0.6)),
    (from=input-2, to=hidden-1, type=stdp, weight=uniform(0.4, 0.6)),
    (from=input-1, to=hidden-2, type=stdp, weight=uniform(0.4, 0.6)),
    (from=input-2, to=hidden-2, type=stdp, weight=uniform(0.4, 0.6)),
    (from=input-1, to=hidden-3, type=stdp, weight=uniform(0.4, 0.6)),
    (from=input-2, to=hidden-3, type=stdp, weight=uniform(0.4, 0.6)),
    (from=hidden-1, to=output-1, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-2, to=output-1, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-3, to=output-1, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-1, to=output-2, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-2, to=output-2, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-3, to=output-2, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-1, to=output-3, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-2, to=output-3, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=hidden-3, to=output-3, type=stdp_2, weight=uniform(0.4, 0.6)),
    (from=output-1, to=inhib-1, type=fixed, weight=2),
    (from=output-2, to=inhib-2, type=fixed, weight=2),
    (from=output-3, to=inhib-3, type=fixed, weight=2),
    (from=inhib-1, to=[output-2, output-3], type=fixed_2, weight=1),
    (from=inhib-2, to=[output-1, output-3], type=fixed_2, weight=1),
    (from=inhib-3, to=[output-1, output-2], type=fixed_2, weight=1),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.08, depression_time=30 ms, potentiation=0.04, potentiation_time=20 ms),
],
)
