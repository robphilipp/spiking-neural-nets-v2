# Running a network over several machines

A session can spread its network over other machines' cores. One `spikes-server` is the **coordinator**: the UI
and API talk to it, and it keeps the clock, the live views, recordings and sensors. Each other machine runs a
**node agent**, which builds its share of the network when the coordinator asks.

```sh
# on each helper machine (trusted network only: node agents take no token)
spikes-server --node --listen 0.0.0.0:7400

# on the coordinator, as usual
spikes-server --listen 0.0.0.0:7070 --token-file token --ui apps/spikes-ui/dist
```

In the UI, set **Settings › Cluster › Nodes** to the agents, e.g. `10.0.0.5:7400, 10.0.0.6:7400`, then build on
the Run page. Every node runs the Run page's shard count. Over the API, pass `nodes` in `build`'s settings.

## How it works

- Every node builds the same network from the same description and keeps only its own neurons: the network is
  split into 64 spatial partitions, spread over the nodes, then over each node's shards. A neuron's ID doesn't depend
  on where it runs, so results don't either.
- Spikes between nodes go directly from node to node over TCP, batched per synchronization window. Dopamine (and
  any modulator) reaches every node.
- Every node's spikes, membrane traces, weight changes and system notices are forwarded to the coordinator, so the
  usual views and recordings show the whole network.
- Requests about one neuron (inspecting it, observing it, connecting or pruning) go to the node that has it.

## Lockstep or best effort

The cluster panel on the Run page has a **Lockstep (exact)** switch.

- **Lockstep** (the default): every window waits for every node, and a node starts a window only once every spike
  sent to it has arrived. Results are exactly those of a single machine, to the nanosecond. Each window costs a
  network round trip, so this is slower across a network. Runs as fast as possible always use lockstep.
- **Best effort**: each node follows its own copy of the coordinator's clock (speed, pause, step and resume are
  sent to every node). A spike that arrives after its time is delivered at once and counted. The panel shows each
  link's share of late spikes, and how late they were (99th percentile and worst). Slower runs give spikes more time
  to cross, so lateness falls as the speed drops. Machines need synchronized clocks (NTP; better, PTP): an offset
  between two clocks shows up as lateness on their links.

## Containers

The `Dockerfile` builds one image with the server and the UI; the same image runs a node agent:

```sh
docker build -t spikes .
docker run --rm -p 7070:7070 -v "$PWD/token:/run/secrets/spikes-token:ro" -v spikes-data:/data spikes
docker run --rm --network host spikes --node --listen 0.0.0.0:7400
```

Nodes open connections to each other on ports chosen when a network is built, so cluster containers need host
networking. `docker-compose.cluster.yml` starts a coordinator and two agents on one host to try it out.

## Limits

- A node that fails or disconnects ends the run with an error; there is no recovery or checkpointing yet.
- Every node holds the network's description and wiring (not just its share), and the coordinator lists every
  synapse: fine for a few machines, not yet for networks of hundreds of millions of neurons.
- All nodes run the same number of shards.
- Clock offsets between machines aren't estimated or corrected; best effort relies on NTP or PTP.
