# MAC ↔ WIN 跨平台剪贴板同步工具

你是否在 Mac 和 Windows 之间来回切换时，被剪贴板不同步的问题搞得焦头烂额？辛苦在 Mac 上复制的一段代码，切换到 Windows 按下 Ctrl+V 却发现空空如也；或者在 Windows 上截了一张图，想粘贴到 Mac 上却还要通过微信/QQ 中转。这些琐碎的操作每天都在消耗你的耐心和时间。

**现在，有了这款工具，一切都不一样了。** 它会在后台默默运行，帮你把 Mac 和 Windows 的剪贴板实时同步——无论是复制的文字还是截图，都能瞬间出现在另一台电脑上。就像它们共用了一个剪贴板一样自然。**（当然你的iphone也会实时同步）**
坐下来，复制，粘贴，就这么简单。

Mac ↔ Windows 局域网剪贴板实时同步，支持**文本**和**图片**。


## 功能特性

- 文本/图片双向同步
- 断线自动重连
- 防回环保护

## 依赖

| 依赖 | 版本 | 说明 |
|------|------|------|
| Python | ≥ 3.9 | 运行环境 |
| websockets | ≥ 12.0 | WebSocket 通信 |
| Pillow | ≥ 10.0 | 图片格式转换 |
| pyobjc-framework-Cocoa | latest | **仅 Mac**，读取剪贴板图片 |
| pywin32 | latest | **仅 Windows**，读写剪贴板 |

## 注意事项

1. **网络要求**：Mac 和 Windows 必须在同一局域网，且 UDP 端口 8766 和 TCP 端口 8765 不被防火墙拦截
2. **Python 路径**：在mac端修改`setup_mac.sh` 中的Python路径，在win端修改`setup_windows.bat` 中的`CONDA_ROOT`和`CONDA_ENV`
3. **macOS 权限**：首次运行时，终端/IDE 需要辅助功能权限才能读取剪贴板
4. **Windows 编译**：`pywin32` 可能需要 Visual C++ 编译环境

## 快速开始

### 1. 安装

**Mac 端**（一键安装）：

```bash
cd clipboard_sync
bash setup_mac.sh
```

该脚本会自动执行：安装 Python 依赖 → 注册 LaunchAgent 开机自启 → 立即启动服务。

**Windows 端**（一键安装）：

```cmd
cd clipboard_sync
setup_windows.bat
```

该脚本会自动执行：安装 Python 依赖 → 创建启动文件夹快捷方式 → 立即启动客户端。

### 2. 手动运行

**Mac 端**（服务器）：

```bash
python clipboard_sync.py --mode server
```

**Windows 端**（客户端）：

```cmd
python clipboard_sync.py --mode client
```

客户端会自动通过 UDP 广播发现 Mac 服务器，无需指定 IP。

### 3. 高级选项

```bash
# 指定端口
python clipboard_sync.py --mode server --port 9090

# Client 手动指定 IP（跳过自动发现）
python clipboard_sync.py --mode client --host 192.168.1.100

# 指定 IP + 端口
python clipboard_sync.py --mode client --host 192.168.1.100 --port 9090
```

## 文件结构

```
clipboard_sync/
├── clipboard_sync.py          # 主程序（Server/Client 模式、UDP 发现、WebSocket）
├── clipboard_mac.py           # Mac 剪贴板读写模块（pbpaste/pbcopy + AppKit）
├── clipboard_win.py           # Windows 剪贴板读写模块（win32clipboard）
├── setup_mac.sh               # Mac 一键安装脚本
├── setup_windows.bat          # Windows 一键安装脚本
├── com.user.clipboard-sync.plist  # macOS LaunchAgent 配置
└── requirements.txt           # Python 依赖
```

## 日志

- **Mac**：日志输出到 `/tmp/clipboard-sync.log`
  ```bash
  tail -f /tmp/clipboard-sync.log
  ```
- **Windows**：日志输出到控制台

## 服务管理

**Mac：**

```bash
# 查看状态
launchctl list | grep clipboard-sync

# 停止服务
launchctl unload ~/Library/LaunchAgents/com.user.clipboard-sync.plist

# 启动服务
launchctl load ~/Library/LaunchAgents/com.user.clipboard-sync.plist

# 卸载（彻底移除）
launchctl unload ~/Library/LaunchAgents/com.user.clipboard-sync.plist
rm ~/Library/LaunchAgents/com.user.clipboard-sync.plist
```

**Windows：**

```cmd
# 停止
taskkill /f /im python.exe

# 删除开机自启
del "%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\clipboard_sync.vbs"
```
