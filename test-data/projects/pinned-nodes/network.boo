// Placement on nodes: a row of sensor neurons pinned to a machine named "edge", feeding a cortex that is spread over
// every machine.
//
// To run it on a cluster, start node agents (`spikes-server --node --listen host:port`) and list them in Settings,
// naming one "edge": for example `10.0.0.2:7091, edge=10.0.0.3:7091`. The sensors then run on the edge node, the
// cortex is spread over all three, and the Deployment page shows where everything landed. Built on the engine alone,
// the pin is ignored and everything runs here.
(
nodes=[(name=edge)],
neuron_types=[(name=cell, threshold=5 mV, membrane_noise=0 mV, conduction_speed=1 m/s)],
populations=[
    (name=sensors, type=cell, count=20, line=(from=[0, 0, 0], to=[190, 0, 0]), node=edge),
    (name=cortex, type=cell, count=200, random=(box=(corner=[0, 50, 0], size=[400, 400, 100]))),
],
connections=[
    (from=sensors, to=cortex, pattern=fan_in, count=5, weight=2),
    (from=cortex, to=cortex, pattern=fan_in, count=10, weight=0.5),
],
)
