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
    (name=output, threshold=6.5 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=25 ms, spike_potential=1 mV, conduction_speed=1 m/s),
],
neurons=[
    (name=output-1, type=output, position=[-30 µm, 0 µm, 20 µm]),
    (name=output-2, type=output, position=[-20 µm, 0 µm, 20 µm]),
    (name=output-3, type=output, position=[-10 µm, 0 µm, 20 µm]),
    (name=output-4, type=output, position=[0 µm, 0 µm, 20 µm]),
    (name=output-5, type=output, position=[10 µm, 0 µm, 20 µm]),
    (name=output-6, type=output, position=[20 µm, 0 µm, 20 µm]),
    (name=output-7, type=output, position=[30 µm, 0 µm, 20 µm]),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.035, depression_time=4 s, potentiation=0.015, potentiation_time=2 s),
],
stickiness=[
    (name=sig, time=10 s),
],
)
