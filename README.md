# Rust Cross-Platform Clipboard Sync

Peer-to-peer clipboard synchronization for macOS, Windows, and Linux. Every running instance is an equal peer: it discovers other devices on the LAN, opens direct WebSocket connections, and synchronizes text and PNG images without a central Hub.

## Features

- Direct peer-to-peer connections
- UDP discovery with manual peer fallback
- Text and PNG image synchronization
- Duplicate connection rejection based on device IDs
- Event IDs and bounded deduplication to prevent loops
- Lamport ordering for deterministic concurrent updates
- Binary protocol with payload hashes and a 16 MiB limit
- Linux X11/XWayland and Wayland clipboard support through `arboard`

## Requirements

- Rust stable toolchain
- macOS, Windows, or Linux desktop session
- Linux: X11/XWayland or a Wayland compositor supporting data-control
- LAN access to TCP `8765` and UDP `8766`

## Run

Start a peer with automatic discovery:

```bash
cargo run --release
```

Connect manually when discovery is unavailable:

```bash
cargo run --release -- --peer 192.168.1.100:8765
cargo run --release -- --discovery false --peer 192.168.1.100:8765
```

Use `--bind 0.0.0.0:8765` to select another listening port and `--device-id` to provide a stable device identity. The device with the lower ID establishes the connection, while the other peer accepts it.

## Development

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
```

The implementation is organized around a few deep modules: `src/protocol.rs` validates the wire format, `src/sync_engine.rs` handles ordering and deduplication, `src/network.rs` manages peer sessions and discovery, and `src/clipboard.rs` contains the cross-platform clipboard Adapter.

GitHub Actions runs format, Clippy, tests, and release builds on Ubuntu, macOS, and Windows. Unit tests do not require a live desktop clipboard; real clipboard permissions and Wayland compositor behavior must be smoke-tested on the target machines.

## Security

The current LAN protocol is plaintext and unauthenticated. Run it only on a trusted private network. Authentication, pairing, and encrypted transport are required before exposing the listener to an untrusted network.
