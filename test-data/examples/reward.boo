// Reward learning: dopamine turns spike timing into learning.
//
// Two inputs drive an output neuron. Their STDP pairings only mark the synapses (an eligibility trace that fades
// over ~1 s); the weights change only if dopamine arrives while the marks last. Turn on background input to the
// "in-" neurons, run, inspect "out", and press "Release dopamine": the weights step up.
(
modulators=[(name=dopamine, clearance_time=300 ms, latency=1 ms)],
neuron_types=[(name=input, threshold=5 mV, membrane_noise=0 mV, conduction_speed=1 m/s)],
neurons=[
    (name=in-1, type=input, position=[0, 0, 0]),
    (name=in-2, type=input, position=[0, 50, 0]),
    (name=out, threshold=0.8 mV, membrane_noise=0 mV, refractory_period=5 ms, position=[100, 25, 0],
        receptors=[(modulator=dopamine, eligibility_time=1 s, learning_rate=20)]),
],
synapse_types=[(name=rewarded, weight=1, learning=stdp, limits=(min=0, max=5), release=())],
connections=[(from=[in-1, in-2], to=out, type=rewarded)],
learning_rules=[(name=stdp, rule=stdp_hard, depression=0.005, depression_time=20 ms, potentiation=0.02, potentiation_time=20 ms)],
)
