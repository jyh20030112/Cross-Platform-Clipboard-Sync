#!/bin/bash
# Mac 端一键安装：安装依赖 + 注册开机自启服务

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLIST_NAME="com.user.clipboard-sync.plist"
PLIST_SRC="$SCRIPT_DIR/$PLIST_NAME"
PLIST_DST="$HOME/Library/LaunchAgents/$PLIST_NAME"

echo "=== 剪贴板同步工具 - Mac 安装 ==="

# 1. 安装 Python 依赖
echo "[1/3] 安装 Python 依赖..."
YOUR_PYTHON_ENV/bin/pip install websockets Pillow pyobjc-framework-Cocoa

# 2. 复制 plist 到 LaunchAgents
echo "[2/3] 注册开机自启服务..."
cp "$PLIST_SRC" "$PLIST_DST"

# 3. 加载服务（立即启动）
echo "[3/3] 启动服务..."
launchctl unload "$PLIST_DST" 2>/dev/null || true
launchctl load "$PLIST_DST"

echo ""
echo "=== 安装完成 ==="
echo "服务已启动，日志文件: /tmp/clipboard-sync.log"
echo ""
echo "常用命令："
echo "  查看日志:   tail -f /tmp/clipboard-sync.log"
echo "  停止服务:   launchctl unload ~/Library/LaunchAgents/$PLIST_NAME"
echo "  启动服务:   launchctl load ~/Library/LaunchAgents/$PLIST_NAME"
echo "  卸载服务:   launchctl unload ~/Library/LaunchAgents/$PLIST_NAME && rm ~/Library/LaunchAgents/$PLIST_NAME"
