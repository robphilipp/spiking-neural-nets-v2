// Populations and connection patterns: 4,000 neurons and 220,000 connections, without listing any of them.
//
// An excitatory and an inhibitory population fill a slab of cortex-like tissue. Each excitatory neuron receives
// 50 inputs from excitatory neurons anywhere and, with probability 0.2, from each inhibitory neuron within 150 µm;
// each inhibitory neuron receives 40 inputs from excitatory neurons within 200 µm. Turn on background input to
// "exc-" neurons and run: activity spreads and is held in check by inhibition.
(
neuron_types=[
    (name=pyramidal, threshold=10 mV, decay_time=20 ms, refractory_period=4 ms, conduction_speed=1 m/s),
    (name=basket, extends=pyramidal, inhibitory=true, threshold=8 mV, decay_time=10 ms, refractory_period=2 ms),
],
synapse_types=[
    (name=excitatory, weight=uniform(0.2, 0.6), learning=stdp, release=()),
    (name=inhibitory, weight=uniform(1, 2), release=()),
],
populations=[
    (name=exc, type=pyramidal, count=3200, random=(box=(corner=[0, 0, 0], size=[800, 800, 200]))),
    (name=inh, type=basket, count=800, random=(box=(corner=[0, 0, 0], size=[800, 800, 200])), seed=2),
],
connections=[
    (from=exc, to=exc, pattern=fan_in, count=50, type=excitatory),
    (from=inh, to=exc, pattern=random, probability=0.2, within=150 µm, type=inhibitory),
    (from=exc, to=inh, pattern=fan_in, count=40, within=200 µm, type=excitatory, weight=uniform(0.5, 1), learning=flat),
],
learning_rules=[(name=stdp, rule=stdp_hard, depression=0.01, depression_time=30 ms, potentiation=0.01, potentiation_time=20 ms)],
)
