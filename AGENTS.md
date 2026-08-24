# Repository Guidelines

## Project Structure & Module Organization

This is a Rust-only Cargo application. `src/main.rs` owns the CLI and runtime; `src/protocol.rs` defines the binary wire protocol; `src/sync_engine.rs` handles Lamport ordering and loop prevention; `src/network.rs` handles peer discovery and WebSocket connections; and `src/clipboard.rs` is the cross-platform clipboard Adapter. Rust tests live beside their modules and future integration tests belong in `tests/`.

## Build, Test, and Development Commands

Use the stable Rust toolchain. The normal development commands are:

```bash
cargo fmt --all
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Run `cargo run --release` on every device. Use `--peer HOST:8765` for manual pairing or `--discovery false` to disable UDP discovery. The LAN must permit TCP 8765 and UDP 8766. Linux builds include X11/XWayland and Wayland clipboard support, subject to the desktop environment.

## Coding Style & Naming Conventions

Use `cargo fmt` and idiomatic Rust: four-space indentation, `snake_case` functions/modules, `CamelCase` types, and `SCREAMING_SNAKE_CASE` constants. Keep platform details behind the `ClipboardBackend` seam. Use `tracing`; never log clipboard contents or image payloads. Validate protocol version, UTF-8 text, PNG signatures, payload length, event digest, and event IDs at the protocol seam.

## Testing Guidelines

Unit tests cover protocol validation, deterministic version ordering, duplicate events, stale-event forwarding, and image bounds. Add tests for every new edge case. Changes to clipboard adapters require manual validation on the relevant OS; transport changes require testing discovery, duplicate connections, reconnects, peer loss, text/images, and firewall failure. CI runs format, Clippy with warnings denied, tests, and release builds on Ubuntu, macOS, and Windows.

## Commit & Pull Request Guidelines

Existing commits use short imperative subjects, including conventional prefixes such as `feat:` (for example, `feat: 跨平台剪贴板同步工具初始版本`). Keep commits focused and run the full local checks before opening a PR. Describe affected platforms, manual/automated verification, dependency or CI changes, and any firewall or permission requirements. Never commit machine-specific paths, credentials, or clipboard data. The current Rust transport is intended for a trusted LAN; do not expose it publicly until authentication and encryption are added.
