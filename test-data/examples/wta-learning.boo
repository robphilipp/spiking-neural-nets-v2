// Winner-takes-all learning: out-1 comes to fire in lockstep with in-1, and out-2 with in-2.
//
// Each output receives both inputs through plastic synapses, all too weak to make it fire on their own; the
// matching ones (in-1 → out-1, in-2 → out-2) start a little stronger. The outputs inhibit each other, so only the
// one that fires first answers. STDP strengthens the synapses that made an output fire, until each input spike
// fires its output by itself (lockstep); the synapses that don't take part decay away. Driving one input at a time
// keeps the two from mixing.
//
// To train it (Inputs page): neurons starting with "in-", rate (Poisson) encoding, 100 Hz at 1, 10 mV per event,
// formula  floor(abs(sin(floor(t/300) * 12.9898) * 43758.5453)) % n == c , 60 s, Inject, then run on the Run page
// as fast as possible. Inspect out-1 and out-2: their incoming weights separate within a few seconds.
(
neuron_types=[
    (name=input, threshold=5 mV, membrane_noise=0 mV, refractory_period=5 ms, conduction_speed=1 m/s),
    (name=output, threshold=1.5 mV, membrane_noise=0 mV, rise_time=0.5 ms, refractory_period=2 ms, decay_time=20 ms,
        conduction_speed=1 m/s),
],
neurons=[
    (name=in-1, type=input, position=[0, 0, 0]),
    (name=in-2, type=input, position=[0, 50, 0]),
    (name=out-1, type=output, position=[100, 0, 0]),
    (name=out-2, type=output, position=[100, 50, 0]),
],
synapse_types=[
    (name=plastic, weight=uniform(0.4, 0.6), equilibrium=0, learning=stdp, limits=(min=0, max=3), decay=(time=30 s)),
    (name=inhibition, weight=10, sign=inhibitory),
],
connections=[
    // The matching pairs start a little stronger: that decides which output learns which input.
    (from=in-1, to=out-1, type=plastic, weight=uniform(1, 1.2)),
    (from=in-2, to=out-2, type=plastic, weight=uniform(1, 1.2)),
    (from=in-1, to=out-2, type=plastic),
    (from=in-2, to=out-1, type=plastic),
    (from=[out-1, out-2], to=[out-1, out-2], type=inhibition),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.012, depression_time=20 ms, potentiation=0.01, potentiation_time=20 ms),
],
)
