# p2p-protos
[![CI](https://github.com/cbass-d/p2p-protos/actions/workflows/ci.yml/badge.svg)](https://github.com/cbass-d/p2p-protos/actions/workflows/ci.yml)

A multi-node libp2p simulator with a terminal UI. Spin up a local swarm of peers, connect and disconnect them on the fly, watch the Kademlia DHT and mDNS discovery in action, and inspect protocol traffic.

Built to learn distributed-systems primitives by running them, not reading about them.

![demo](docs/demo.gif)

## What it does

- **Run a local swarm** of up to 10 nodes on either an in-process `MemoryTransport` or real `TCP` (with noise + yamux).
- **Drive each node from the TUI** — connect / disconnect / start / stop nodes, inspect their state, see live protocol traffic.
- **Implements four libp2p protocols:**
  - **Kademlia** — bootstrap, get/put records, list local records, peer routing
  - **Identify** — peer info exchange
  - **mDNS** — local peer discovery
  - **Swarm** — connection lifecycle
- **Structured tracing** with logs and per-target log levels.

## Architecture

```
TUI ──events──▶ NodeNetwork ──commands──▶ Node ──┐
 ▲                  │                            │
 │                  ▼                            ▼
 └──────────── EventBus ◀─────── effects ◀── handlers
```

- **Typed event bus** (`tokio::broadcast`-backed) fans `NetworkEvent`s out to every subscriber — TUI, observers, tests.
- **Per-protocol handler chain** (`SwarmEventHandler` trait): swarm events flow through an ordered chain; the first handler returning `Ok(true)` claims the event and produces an `Effects` struct.
- **Effects-as-data**: handlers don't perform I/O — they describe what the node should do (dials, kad operations, state mutations, events to publish). `RunningNode::apply_effects` is the single piece of code that runs them.
- **Cooperative cancellation** via `CancellationToken` for clean shutdown across all spawned tasks.

This split makes each piece independently testable and the data flow easy to follow.

## Build

```
cargo build --release
```

## Run

```
p2p-protos [OPTIONS] <TRANSPORT_MODE> [BIND_ADDRESS]
```

| Argument | Description |
|---|---|
| `<TRANSPORT_MODE>` | `memory` (in-process) or `tcp` |
| `[BIND_ADDRESS]` | Required when `TRANSPORT_MODE=tcp`. IPv4 (e.g. `127.0.0.1`). |
| `-n, --nodes <N>` | Number of nodes to start (default `5`, max `10`) |

Examples:

```
p2p-protos -n 3 memory
p2p-protos -n 5 tcp 127.0.0.1
```

## TUI controls

Three panels: a node visualization box, a node list, and a log viewer.

- `Tab` — switch focus between the node list and the log viewer
- `Esc` — close popups
- `q` — quit
- Interact with nodes (connect, disconnect, start, stop, view info) through popup menus on the node list.

## Logging

Logs are written to `logs/p2p.log` (rotated daily). Override the level with `RUST_LOG`:

```
RUST_LOG=debug cargo run -- memory
RUST_LOG=p2p_protos=trace,libp2p=info cargo run -- memory
```

## Tech stack

`tokio` · `libp2p` (kad, identify, mdns, noise, yamux, tcp) · `ratatui` · `tracing` · `clap` · `thiserror`
