# Notes on the original Spikes project (v1)

These notes summarize the original project in `/Users/rob/code/spiking-neural-nets` as context for the v2
rebuild (Rust backend, Tauri + TypeScript UI). They were written on 2026-11-22 from a read-through of the code,
wiki, paper draft and UI.

| File | What it covers |
|------|----------------|
| [01-overview-and-history.md](01-overview-and-history.md) | What Spikes is, the directory map, and how the pieces evolved |
| [02-neuron-model.md](02-neuron-model.md) | The neuron and synapse equations and the event-driven update algorithm |
| [03-network-description-language.md](03-network-description-language.md) | The `.boo` "DNA" network description, sensor files and project files |
| [04-v1-runtime-architecture.md](04-v1-runtime-architecture.md) | How v1 runs: Akka actors, timing and the scheduler, distribution, logging, server |
| [05-v1-ui.md](05-v1-ui.md) | The Electron/React UI and what it does |
| [06-lessons-and-v2-implications.md](06-lessons-and-v2-implications.md) | v1 pain points, what to keep, and the options for the v2 web layer |

The actor framework plan is in `../docs/plans/actors/`.
