// line sensor network
// For parameters that accept units, if they are not specified, they default to:
// • distances to µm
// • times to ms
// • conductance speeds to m/s
// • electric potentials to mV
// • frequencies to Hz
// • magnetic flux to Wb
// notes
// • wnm from 1e-3 to 0
// • ipl from 0.001 to 0.00 for output layer
// • mpn from 0.05 to 0.0
(
neuron_types=[
    (name=in, threshold=1 mV, refractory_period=2 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=2500 ms, rise_time=2 ms, membrane_noise=0 mV, spike_potential=1.1 mV, conduction_speed=0.1 m/s, intrinsic_plasticity=(step=0 mV, amplitude=0 mV, decay_time=3600 s)),
    (name=inh, threshold=0.4 mV, inhibitory=true, refractory_period=0.1 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=250 ms, rise_time=2 ms, membrane_noise=0 mV, spike_potential=0.5 mV, conduction_speed=0.08 m/s, intrinsic_plasticity=(step=0 mV, amplitude=0 mV, decay_time=3600 s)),
    (name=out, threshold=1 mV, refractory_period=20 ms, relative_refractoriness=0.1 µWb, resting_potential=0 mV, decay_time=2500 ms, rise_time=2 ms, membrane_noise=0 mV, spike_potential=1 mV, conduction_speed=1 m/s, intrinsic_plasticity=(step=0 mV, amplitude=0 nV, decay_time=3600 s)),
],
synapse_types=[
    (name=stdp_alpha, limits=(min=0, max=1), release=(facilitation_base=1000, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=1000, depletion_step=10, depletion_time=100 ms), weight_noise=0.00001, learning=stdp_alpha),
    (name=fixed, decay=(time=10 s), limits=(min=0, max=1.5), release=(facilitation_base=1000, facilitation_step=0, facilitation_time=100 ms, depletion_base=1000, depletion_step=0, depletion_time=100 ms)),
    (name=fixed_2, limits=(min=0, max=1), release=(facilitation_base=1000, facilitation_step=0.1, facilitation_time=100 ms, depletion_base=1000, depletion_step=10, depletion_time=100 ms), weight_noise=0.00001),
],
neurons=[
    (name=in-1, type=in, position=[-300 µm, 0 µm, 100 µm]),
    (name=in-2, type=in, position=[300 µm, 0 µm, 100 µm]),
    (name=inh-1, type=inh, position=[-290 µm, 0 µm, 0 µm]),
    (name=inh-2, type=inh, position=[290 µm, 0 µm, 0 µm]),
    (name=out-1, type=out, position=[-300 µm, 0 µm, 0 µm]),
    (name=out-2, type=out, position=[300 µm, 0 µm, 0 µm]),
],
connections=[
    (from=[in-1, in-2], to=[out-1, out-2], type=stdp_alpha, weight=0.5),
    (from=out-1, to=inh-1, type=fixed, weight=1),
    (from=out-2, to=inh-2, type=fixed, weight=1),
    (from=inh-1, to=out-2, type=fixed_2, weight=1),
    (from=inh-2, to=out-1, type=fixed_2, weight=1),
],
learning_rules=[
    (name=stdp_soft, rule=stdp_soft, depression=0.06, depression_time=15 ms, potentiation=0.02, potentiation_time=10 ms),
    (name=stdp_alpha, rule=stdp_alpha, baseline=-1, rate=0.04, time=22 ms),
],
)
