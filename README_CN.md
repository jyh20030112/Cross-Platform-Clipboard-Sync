# Rust 跨平台剪贴板同步

基于 Rust 的 macOS、Windows、Linux 局域网 P2P 剪贴板同步工具。每个运行中的实例都是平等的 Peer：自动发现局域网设备，建立直接 WebSocket 连接，不依赖中心 Hub

## 功能

- P2P 直连，不存在中心服务器
- UDP 自动发现，支持手动指定 Peer
- 文本和 PNG 图片同步
- 根据设备 ID 拒绝重复连接
- 事件 ID 和有限去重缓存，防止消息回环
- Lamport 版本排序，保证并发更新最终一致
- 二进制协议、payload hash 校验、16 MiB 大小限制
- 通过 `arboard` 支持 Linux X11/XWayland 和 Wayland

## 环境要求

- Rust stable 工具链
- macOS、Windows 或 Linux 桌面环境
- Linux 需要 X11/XWayland，或支持 data-control 的 Wayland 合成器
- 局域网允许 TCP `8765` 和 UDP `8766`

## 运行

自动发现并启动 Peer：

```bash
cargo run --release
```

网络禁止 UDP 广播时手动连接：

```bash
cargo run --release -- --peer 192.168.1.100:8765
cargo run --release -- --discovery false --peer 192.168.1.100:8765
```

使用 `--bind 0.0.0.0:8765` 修改监听地址，使用 `--device-id` 指定稳定设备 ID。每对设备中，ID 较小的一方主动建立连接，另一方接受连接

## 开发检查

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
```

代码按深模块划分：`src/protocol.rs` 负责协议校验，`src/sync_engine.rs` 负责版本排序和去重，`src/network.rs` 负责 Peer 连接与发现，`src/clipboard.rs` 负责跨平台剪贴板 Adapter。

GitHub Actions 会在 Ubuntu、macOS、Windows 上运行格式检查、Clippy、测试和 Release 构建。单元测试不依赖真实桌面剪贴板；实际剪贴板权限和 Wayland 合成器行为仍需在目标设备上进行冒烟测试。

## 安全说明

当前局域网协议未加密且没有认证，只能运行在可信的私有网络中。在开放网络前必须加入设备认证、配对和加密传输。
