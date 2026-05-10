# MAC ↔ WIN Aross-Platform Clipboard Sync

Ever been frustrated switching between Mac and Windows, only to find your clipboard didn't follow you? You copy a block of code on your Mac, switch to Windows, hit Ctrl+V, and — nothing. Or you take a screenshot on Windows, then have to send it through WeChat/Telegram just to paste it on your Mac. These little frictions eat away at your patience and productivity, every single day.

**That ends here.** This tool runs quietly in the background, keeping your clipboard in sync in real time. Text you copy, screenshots you take — they appear instantly on the other machine, as if both computers share one clipboard.**（Of course, your iPhone will also sync in real time.）**

Sit down, copy, paste. It's that simple.

Real-time LAN clipboard sync between Mac and Windows. Supports **text** and **images**.

## Features

- Bidirectional text and image sync
- Auto-reconnect on connection loss
- Anti-loopback protection

## Dependencies

| Dependency | Version | Notes |
|------------|---------|-------|
| Python | ≥ 3.9 | Runtime |
| websockets | ≥ 12.0 | WebSocket communication |
| Pillow | ≥ 10.0 | Image format conversion |
| pyobjc-framework-Cocoa | latest | **Mac only**, clipboard image access |
| pywin32 | latest | **Windows only**, clipboard API |

## Notes

1. **Network**: Both machines must be on the same LAN, with UDP port 8766 and TCP port 8765 allowed through the firewall
2. **Python path**: On Mac, update the Python path in `setup_mac.sh`; on Windows, update `CONDA_ROOT` and `CONDA_ENV` in `setup_windows.bat`
3. **macOS permissions**: On first run, Terminal/IDE may need Accessibility permission to read the clipboard
4. **Windows build tools**: `pywin32` may require a Visual C++ build environment

## Quick Start

### 1. Installation

**Mac** (one-click setup):

```bash
cd clipboard_sync
bash setup_mac.sh
```

This script will: install Python dependencies → register LaunchAgent for auto-start → start the service immediately.

**Windows** (one-click setup):

```cmd
cd clipboard_sync
setup_windows.bat
```

This script will: install Python dependencies → create a Startup folder shortcut → start the client immediately.

### 2. Manual Run

**Mac** (server):

```bash
python clipboard_sync.py --mode server
```

**Windows** (client):

```cmd
python clipboard_sync.py --mode client
```

The client auto-discovers the Mac server via UDP broadcast — no IP needed.

### 3. Advanced Options

```bash
# Custom port
python clipboard_sync.py --mode server --port 9090

# Client with manual IP (skip auto-discovery)
python clipboard_sync.py --mode client --host 192.168.1.100

# Manual IP + port
python clipboard_sync.py --mode client --host 192.168.1.100 --port 9090
```

## File Structure

```
clipboard_sync/
├── clipboard_sync.py          # Main engine (server/client, UDP discovery, WebSocket)
├── clipboard_mac.py           # Mac clipboard I/O (pbpaste/pbcopy + AppKit)
├── clipboard_win.py           # Windows clipboard I/O (win32clipboard)
├── setup_mac.sh               # Mac one-click setup script
├── setup_windows.bat          # Windows one-click setup script
├── com.user.clipboard-sync.plist  # macOS LaunchAgent config
└── requirements.txt           # Python dependencies
```

## Logs

- **Mac**: Logs written to `/tmp/clipboard-sync.log`
  ```bash
  tail -f /tmp/clipboard-sync.log
  ```
- **Windows**: Logs printed to console

## Service Management

**Mac:**

```bash
# Check status
launchctl list | grep clipboard-sync

# Stop
launchctl unload ~/Library/LaunchAgents/com.user.clipboard-sync.plist

# Start
launchctl load ~/Library/LaunchAgents/com.user.clipboard-sync.plist

# Uninstall
launchctl unload ~/Library/LaunchAgents/com.user.clipboard-sync.plist
rm ~/Library/LaunchAgents/com.user.clipboard-sync.plist
```

**Windows:**

```cmd
# Stop
taskkill /f /im python.exe

# Remove auto-start
del "%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\clipboard_sync.vbs"
```

