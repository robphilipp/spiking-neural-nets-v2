// line sensor network
// For parameters that accept units, if they are not specified, they default to:
// • distances to µm
// • times to ms
// • conductance speeds to m/s
// • electric potentials to mV
// • frequencies to Hz
// • magnetic flux to Wb
(
synapse_types=[
    (name=stdp, limits=(min=0, max=5), release=(facilitation_base=1, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=10, depletion_step=1, depletion_time=100 ms), learning=stdp),
],
neurons=[
    (name=input-1, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=2 mV, conduction_speed=1 m/s, position=[1 mm, 0 µm, 0 µm]),
    (name=input-2, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=2 mV, conduction_speed=0.5 m/s, position=[-1 mm, 0 µm, 0 µm]),
    (name=output-1, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0.1 mV, spike_potential=1 mV, conduction_speed=1 m/s, position=[0 µm, 10 mm, 0 µm]),
],
connections=[
    (from=input-1, to=output-1, type=stdp, weight=1),
    (from=input-2, to=output-1, type=stdp, weight=1),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.035, depression_time=40 ms, potentiation=0.015, potentiation_time=20 ms),
],
)
