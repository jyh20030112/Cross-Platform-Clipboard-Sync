#!/usr/bin/env python3
"""
跨平台剪贴板同步工具（Mac ↔ Windows）

使用方式：
  Mac (server):     python clipboard_sync.py --mode server
  Windows (client): python clipboard_sync.py --mode client
  （Client 会通过 UDP 广播自动发现 Server，无需手动指定 IP）
"""
from __future__ import annotations

import argparse
import asyncio
import json
import logging
import platform
import socket
import struct
import sys
import time

import websockets

# 根据操作系统导入对应的剪贴板模块
_system = platform.system()
if _system == "Darwin":
    import clipboard_mac as clipboard
elif _system == "Windows":
    import clipboard_win as clipboard
else:
    print(f"不支持的操作系统: {_system}")
    sys.exit(1)

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%H:%M:%S",
)
log = logging.getLogger("clipboard_sync")

# 防回环标记：记录最后一次从远端写入的时间戳
_last_remote_write_time = 0.0
# 防回环保护时间（秒）：写入后这段时间内不检测本地变化
_REMOTE_WRITE_GUARD = 1.5

# UDP 自动发现常量
_DISCOVERY_PORT = 8766
_DISCOVERY_MAGIC = b"CLIPSYNC"
_DISCOVERY_INTERVAL = 3  # 秒


# ── UDP 自动发现 ──

async def discovery_beacon(ws_port: int, connections: set):
    """Server 端：定期发送 UDP 广播。有客户端连接时暂停，断开后恢复。"""
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    sock.setblocking(False)

    payload = _DISCOVERY_MAGIC + struct.pack("!H", ws_port)
    log.info(f"UDP 广播发现服务已启动 (端口 {_DISCOVERY_PORT})")

    loop = asyncio.get_event_loop()
    while True:
        if connections:
            # 有客户端连接，暂停广播
            await asyncio.sleep(_DISCOVERY_INTERVAL)
            continue
        try:
            await loop.run_in_executor(
                None,
                lambda: sock.sendto(payload, ("255.255.255.255", _DISCOVERY_PORT)),
            )
        except Exception as e:
            log.debug(f"广播发送失败: {e}")
        await asyncio.sleep(_DISCOVERY_INTERVAL)


async def discover_server(timeout: float = 30.0) -> tuple[str, int]:
    """Client 端：监听 UDP 广播，自动发现 Server 的 IP 和端口。"""
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    if hasattr(socket, "SO_REUSEPORT"):
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
    sock.bind(("", _DISCOVERY_PORT))
    sock.setblocking(False)
    sock.settimeout(1.0)

    log.info("正在搜索局域网中的 Server...")

    loop = asyncio.get_event_loop()
    deadline = time.time() + timeout

    while time.time() < deadline:
        try:
            data, addr = await loop.run_in_executor(None, lambda: sock.recvfrom(64))
            if data.startswith(_DISCOVERY_MAGIC) and len(data) == len(_DISCOVERY_MAGIC) + 2:
                ws_port = struct.unpack("!H", data[len(_DISCOVERY_MAGIC):])[0]
                server_ip = addr[0]
                log.info(f"发现 Server: {server_ip}:{ws_port}")
                sock.close()
                return server_ip, ws_port
        except (socket.timeout, BlockingIOError, OSError):
            await asyncio.sleep(0.5)

    sock.close()
    raise TimeoutError(f"在 {timeout} 秒内未发现 Server，请确认 Server 已启动且在同一局域网")


class ClipboardSync:
    def __init__(self):
        self._last_hash = clipboard.get_clipboard_hash()
        self._connections: set = set()
        self._ws_client = None

    # ── 剪贴板监控 ──

    async def monitor_clipboard(self, send_fn):
        """持续监控本地剪贴板变化，变化时调用 send_fn 发送。"""
        global _last_remote_write_time
        log.info("开始监控本地剪贴板...")

        while True:
            await asyncio.sleep(0.5)

            try:
                current_hash = clipboard.get_clipboard_hash()
                if current_hash == self._last_hash:
                    continue

                self._last_hash = current_hash

                # 防回环：如果刚刚写入了远端内容，跳过
                if time.time() - _last_remote_write_time < _REMOTE_WRITE_GUARD:
                    continue

                content_type, content = clipboard.get_clipboard_content()
                if content_type == "empty" or content is None:
                    continue

                msg = json.dumps({
                    "type": content_type,
                    "content": content,
                })

                # 图片日志不打印完整 base64
                if content_type == "image":
                    log.info(f"检测到本地剪贴板变化: [图片 {len(content)} bytes base64]")
                else:
                    preview = content[:80] + ("..." if len(content) > 80 else "")
                    log.info(f"检测到本地剪贴板变化: {preview}")

                await send_fn(msg)

            except Exception as e:
                log.error(f"监控剪贴板出错: {e}")

    def handle_remote_message(self, raw_msg: str):
        """处理从远端收到的剪贴板内容。"""
        global _last_remote_write_time

        try:
            msg = json.loads(raw_msg)
            content_type = msg.get("type")
            content = msg.get("content")

            if not content_type or not content:
                return

            success = False
            if content_type == "text":
                success = clipboard.set_text(content)
                preview = content[:80] + ("..." if len(content) > 80 else "")
                log.info(f"收到远端文本: {preview}")
            elif content_type == "image":
                success = clipboard.set_image(content)
                log.info(f"收到远端图片: [{len(content)} bytes base64]")

            if success:
                _last_remote_write_time = time.time()
                # 更新 hash，防止回环
                self._last_hash = clipboard.get_clipboard_hash()
                log.info("已写入本地剪贴板")
            else:
                log.error("写入本地剪贴板失败")

        except json.JSONDecodeError:
            log.error("收到无效的 JSON 消息")
        except Exception as e:
            log.error(f"处理远端消息出错: {e}")

    # ── Server 模式 ──

    async def _server_handler(self, websocket):
        """处理 WebSocket 连接。"""
        remote = websocket.remote_address
        log.info(f"客户端已连接: {remote[0]}:{remote[1]}")
        self._connections.add(websocket)

        try:
            async for message in websocket:
                self.handle_remote_message(message)
                # 转发给其他客户端（如果将来有多个）
                for conn in self._connections:
                    if conn != websocket and conn.state.name == "OPEN":
                        try:
                            await conn.send(message)
                        except Exception:
                            pass
        except websockets.ConnectionClosed:
            log.info(f"客户端断开: {remote[0]}:{remote[1]}")
        finally:
            self._connections.discard(websocket)

    async def _server_broadcast(self, msg: str):
        """广播消息给所有连接的客户端。"""
        for conn in list(self._connections):
            if conn.state.name == "OPEN":
                try:
                    await conn.send(msg)
                except Exception:
                    self._connections.discard(conn)

    async def run_server(self, host: str, port: int):
        """启动 Server 模式。"""
        log.info(f"启动 WebSocket 服务器: {host}:{port}")
        log.info(f"操作系统: {_system}")
        log.info("等待客户端连接...")

        # 同时启动 WebSocket 服务和 UDP 广播
        async with websockets.serve(self._server_handler, host, port):
            beacon_task = asyncio.create_task(discovery_beacon(port, self._connections))
            try:
                await self.monitor_clipboard(self._server_broadcast)
            finally:
                beacon_task.cancel()

    # ── Client 模式 ──

    async def _client_send(self, msg: str):
        """发送消息给服务器。"""
        if self._ws_client and self._ws_client.state.name == "OPEN":
            await self._ws_client.send(msg)

    async def _client_receive(self, websocket):
        """接收服务器消息。"""
        async for message in websocket:
            self.handle_remote_message(message)

    async def run_client(self, host: str | None, port: int):
        """启动 Client 模式。"""
        log.info(f"操作系统: {_system}")

        while True:
            try:
                # 如果未指定 host，自动发现
                if host is None:
                    server_ip, server_port = await discover_server()
                else:
                    server_ip, server_port = host, port

                uri = f"ws://{server_ip}:{server_port}"
                log.info(f"连接到服务器: {uri}")

                async with websockets.connect(uri) as websocket:
                    self._ws_client = websocket
                    log.info("已连接到服务器！剪贴板同步已启动。")

                    # 并行运行：接收消息 + 监控剪贴板
                    receive_task = asyncio.create_task(
                        self._client_receive(websocket)
                    )
                    monitor_task = asyncio.create_task(
                        self.monitor_clipboard(self._client_send)
                    )

                    # 任一任务结束（通常是连接断开）
                    done, pending = await asyncio.wait(
                        [receive_task, monitor_task],
                        return_when=asyncio.FIRST_COMPLETED,
                    )
                    for task in pending:
                        task.cancel()

            except TimeoutError as e:
                log.warning(f"{e}，继续搜索...")
            except (ConnectionRefusedError, OSError) as e:
                log.warning(f"连接失败: {e}，3 秒后重试...")
                await asyncio.sleep(3)
            except websockets.ConnectionClosed:
                log.warning("连接断开，3 秒后重连...")
                await asyncio.sleep(3)


def main():
    parser = argparse.ArgumentParser(
        description="跨平台剪贴板同步工具（Mac ↔ Windows）"
    )
    parser.add_argument(
        "--mode", required=True, choices=["server", "client"],
        help="运行模式：server（Mac）或 client（Windows）",
    )
    parser.add_argument(
        "--host", default=None,
        help="Client 模式：手动指定服务器 IP（留空则自动发现）；Server 模式：监听地址",
    )
    parser.add_argument(
        "--port", type=int, default=8765,
        help="WebSocket 端口号（默认 8765）",
    )
    args = parser.parse_args()

    sync = ClipboardSync()

    try:
        if args.mode == "server":
            server_host = args.host or "0.0.0.0"
            asyncio.run(sync.run_server(server_host, args.port))
        else:
            asyncio.run(sync.run_client(args.host, args.port))
    except KeyboardInterrupt:
        log.info("已停止。")


if __name__ == "__main__":
    main()
