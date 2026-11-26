// Winner-takes-all learning: out-1 learns to fire in lockstep with in-1, and out-2 with in-2.
//
// Each output receives both inputs through plastic synapses, all too weak to make it fire by themselves: at first an
// output answers only about one in ten of its input's spikes, when its potential happens to sum high enough. The
// matching synapses (in-1 → out-1, in-2 → out-2) start a little stronger, which decides which output learns which
// input. The outputs inhibit each other, so only one answers at a time. STDP strengthens the synapses that made an
// output fire, until each input spike fires its output by itself (lockstep, after about 15 to 25 s of training);
// the synapses that don't take part fade. Driving one input at a time keeps the two from mixing.
//
// To train it (Inputs page): neurons starting with "in-", rate (Poisson) encoding, 100 Hz at 1, 10 mV per event,
// formula  floor(abs(sin(floor(t/300) * 12.9898) * 43758.5453)) % n == c , 60 s, Inject before running. Then on the
// Run page, at speed ×1, inspect out-1 and out-2 and Start: watch the raster go from sparse to lockstep while the
// matching weights climb to 3 and the others fade.
(
neuron_types=[
    (name=input, threshold=5 mV, membrane_noise=0 mV, refractory_period=5 ms, conduction_speed=1 m/s),
    (name=output, threshold=1.5 mV, membrane_noise=0.2 mV, rise_time=0.5 ms, refractory_period=2 ms, decay_time=20 ms,
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
    (from=in-1, to=out-1, type=plastic, weight=uniform(0.55, 0.65)),
    (from=in-2, to=out-2, type=plastic, weight=uniform(0.55, 0.65)),
    (from=in-1, to=out-2, type=plastic),
    (from=in-2, to=out-1, type=plastic),
    (from=[out-1, out-2], to=[out-1, out-2], type=inhibition),
],
learning_rules=[
    (name=stdp, rule=stdp_hard, depression=0.012, depression_time=20 ms, potentiation=0.01, potentiation_time=20 ms),
],
)
