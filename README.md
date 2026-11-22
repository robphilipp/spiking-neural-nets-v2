# spiking-neural-nets-v2

Rebuild of the Spikes spiking neural network simulator: a Rust backend built on a timed actor framework, and a
Tauri + TypeScript UI.

## Layout

This is a monorepo. Projects are added as they're needed.

| Path | Contents | Status |
|------|----------|--------|
| `crates/tempo-*` | Timed actor framework (library). Knows nothing about neurons. | `tempo-core` started |
| `crates/spikes-*` | Neuron model, `.boo` parser, network actors, recording, API types (libraries) | planned |
| `apps/` | `spikes-node` (cluster worker), `spikes-server` (coordinator, web API, data-queue adapter), `spikes-ui` (Tauri + TypeScript) | planned |
| `packages/` | Shared TypeScript packages (types generated from the Rust API) | planned |
| `docs/plans/` | Design plans (start with `docs/plans/actors/README.md`) | |
| `notes/` | Notes on the original v1 project | |

## Dependency rule

Dependencies point one way: `tempo-*` → `spikes-*` → apps. `tempo-*` crates must never depend on `spikes-*`
crates, and the UI depends only on the generated API package.

## Build

```sh
cargo build
cargo test
cargo clippy --all-targets
```
