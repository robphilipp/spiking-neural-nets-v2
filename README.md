# spiking-neural-nets-v2

Rebuild of the Spikes spiking neural network simulator: a Rust backend built on a timed actor framework, and a
Tauri + TypeScript UI.

## Layout

This is a monorepo. Projects are added as they're needed.

| Path | Contents | Status |
|------|----------|--------|
| `crates/tempo-*` | Timed actor framework (library). Knows nothing about neurons. | phases 1–3 done, phase 4's core: `tempo-core`, `tempo-queue`, `tempo-runtime` (parallel shards, live clock, control plane, subscriptions, fields, cluster nodes), `tempo-cluster` (TCP mesh, lockstep and best-effort windows), `tempo-testkit` |
| `crates/spikes-*` | Neuron model, `.boo` parser, network actors, recording, API types (libraries) | `spikes-model`, `spikes-dna`, `spikes-network` (with neuromodulation), `spikes-record` done; see `docs/plans/neurons/` |
| `apps/` | `spikes-cli` (`spikes check/run/parquet`), `spikes-server` (HTTP + WebSocket, serves the UI; `--node` for a cluster node agent), `spikes-ui` (web app and Tauri desktop app) | A1–A6 done; see `docs/plans/app/`, and `docs/cluster.md` for clusters |
| `packages/` | `spikes-client` (TypeScript client: protocol types, frames, sensors) | done |
| `clients/` | `python/spikes_sensor` (stream sensor data, microphone example) | done |
| `test-data/networks/` | test networks (v1's, migrated to the current language) | |
| `docs/plans/` | Design plans: `actors/` (framework), `neurons/` (model crates), `app/` (engine, server, Tauri UI) | |
| `notes/` | Notes on the original v1 project | |

## Dependency rule

Dependencies point one way: `tempo-*` → `spikes-*` → apps. `tempo-*` crates must never depend on `spikes-*`
crates, and the UI depends only on the generated API package.

## Build

```sh
cargo build
cargo test
cargo clippy --all-targets
cargo bench -p tempo-queue        # heap vs timing wheel
cargo bench -p tempo-testkit      # runtime: ping-pong, ring, LIF network
cargo run --release -p tempo-runtime --example spawn_many -- 10000000
cargo run --release -p tempo-testkit --example lif_scaling -- 100000 100 50 1,2,4,8   # parallel speedup
cargo run --release -p tempo-testkit --example realtime -- 10000 20 2000 1.0 2        # real-time pacing
cargo run --release -p tempo-testkit --example live                                   # scripted live session
cargo run --release -p spikes-record --example record -- test-data/networks/stdp-test.boo runs/stdp 10   # record a run to Arrow + Parquet
cargo run --release -p spikes-cli -- run test-data/networks/stdp-test.boo --for 10s --poisson input:40:3 --record runs/x --parquet
cargo run --release -p spikes-session --example stream_100k -- 100000 10 4   # 100k neurons, paced, live frames
cargo run --release -p spikes-server                                          # the engine on ws://127.0.0.1:7070
cargo run -p spikes-sensor --example tones                                    # a sensor streaming into it
npm install                                                                   # once, at the root (npm workspaces)
(cd packages/spikes-client && npm test)                                      # TS client tests (needs: cargo build -p spikes-server)
(cd apps/spikes-ui && npm run build && npm run e2e)                          # UI tests in Chromium
(cd apps/spikes-ui && npm run desktop)                                       # the desktop app (dev mode)
(cd apps/spikes-ui && npm run desktop:selftest)                              # desktop self-test
```
