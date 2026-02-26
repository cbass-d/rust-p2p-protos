# p2p-protos

A peer-to-peer networking prototype built with Rust. It spawns a local network of libp2p nodes and provides a terminal UI (TUI) for interacting with them in real time.

The nodes use the Identify and Kademlia protocols. You can connect and disconnect nodes, start and stop them, and inspect protocol-level information, all from the terminal interface.

## Build

```
cargo build --release
```

## Usage

```
./target/release/p2p-protos [OPTIONS]
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `-n, --nodes <N>` | Number of nodes to start | 5 |

### Example

Start a network with 3 nodes:

```
./target/release/p2p-protos -n 3
```

## TUI Controls

The interface has three panels: a node visualization box, a node list, and a log viewer. Press `Tab` to switch focus between the node list and the log viewer. Interact with nodes through popup menus (connect, disconnect, start, stop, view info). Press `Esc` to close popups and `q` to quit.

## Logging

Logs are written to `logs/p2p.log` on a daily rolling basis. Set the log level with the `RUST_LOG` environment variable:

```
RUST_LOG=debug cargo run
```
