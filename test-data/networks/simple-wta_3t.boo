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
    (name=input, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=0.5 mV, conduction_speed=0.1 m/s),
    (name=inhib, threshold=1 mV, inhibitory=true, refractory_period=0.1 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=2 mV, conduction_speed=0.08 m/s),
    (name=output, threshold=2 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=1 mV, conduction_speed=1 m/s),
],
synapse_types=[
    (name=stdp, decay=(time=2 h), limits=(min=0, max=1), release=(facilitation_base=1, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=10, depletion_step=1, depletion_time=100 ms), learning=stdp),
    (name=fixed, decay=(time=2 h), limits=(min=0, max=1), release=(facilitation_base=1000, facilitation_step=0, facilitation_time=100 ms, depletion_base=1000, depletion_step=0, depletion_time=100 ms)),
    (name=fixed_2, decay=(time=2 h), limits=(min=0, max=1), release=(facilitation_base=1, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=10, depletion_step=1, depletion_time=100 ms)),
],
neurons=[
    (name=input-1, type=input, position=(r=0 µm, phi=0, z=100 µm)),
    (name=input-2, type=input, position=(r=0 µm, phi=0, z=100 µm)),
    (name=inhib-1, type=inhib, position=(r=0 µm, phi=0, z=0 µm)),
    (name=inhib-2, type=inhib, position=(r=0 µm, phi=0, z=0 µm)),
    (name=inhib-3, type=inhib, position=(r=0 µm, phi=0, z=0 µm)),
    (name=output-1, type=output, position=(r=300 µm, phi=0, z=0 µm)),
    (name=output-2, type=output, position=(r=300 µm, phi=2.0944, z=0 µm)),
    (name=output-3, type=output, position=(r=300 µm, phi=-2.0944, z=0 µm)),
],
connections=[
    (from=input-1, to=output-1, type=stdp, weight=uniform(0.1, 0.9)),
    (from=input-2, to=output-1, type=stdp, weight=uniform(0.1, 0.9)),
    (from=input-1, to=output-2, type=stdp, weight=uniform(0.1, 0.9)),
    (from=input-2, to=output-2, type=stdp, weight=uniform(0.1, 0.9)),
    (from=input-1, to=output-3, type=stdp, weight=uniform(0.1, 0.9)),
    (from=input-2, to=output-3, type=stdp, weight=uniform(0.1, 0.9)),
    (from=output-1, to=inhib-1, type=fixed, weight=2),
    (from=output-2, to=inhib-2, type=fixed, weight=2),
    (from=output-3, to=inhib-3, type=fixed, weight=2),
    (from=inhib-1, to=[output-2, output-3], type=fixed_2, weight=1),
    (from=inhib-2, to=[output-1, output-3], type=fixed_2, weight=1),
    (from=inhib-3, to=[output-1, output-2], type=fixed_2, weight=1),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.03, depression_time=30 ms, potentiation=0.015, potentiation_time=20 ms),
],
)
