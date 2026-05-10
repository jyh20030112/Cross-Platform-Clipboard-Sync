@echo off
echo === Clipboard Sync - Windows Setup ===

set "CONDA_ROOT=YOUR_CONDA_ROOT_PATH"
set "CONDA_ENV=YOUR_CONDA_ENV_NAME"
set "PYTHON=%CONDA_ROOT%\envs\%CONDA_ENV%\python.exe"
set "PIP=%CONDA_ROOT%\envs\%CONDA_ENV%\Scripts\pip.exe"

echo Using Python: %PYTHON%

echo [1/3] Installing Python dependencies...
"%PIP%" install websockets Pillow pywin32
if errorlevel 1 (
    echo ERROR: pip install failed.
    pause
    exit /b 1
)

echo [2/3] Creating startup script...
set "SCRIPT_DIR=%~dp0"
set "STARTUP_DIR=%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup"
set "VBS_FILE=%STARTUP_DIR%\clipboard_sync.vbs"

> "%VBS_FILE%" echo Set WshShell = CreateObject("WScript.Shell")
>> "%VBS_FILE%" echo WshShell.CurrentDirectory = "%SCRIPT_DIR%"
>> "%VBS_FILE%" echo WshShell.Run """%PYTHON%"" ""%SCRIPT_DIR%clipboard_sync.py"" --mode client", 0, False

echo [3/3] Starting service...
cd /d "%SCRIPT_DIR%"
start "" wscript "%VBS_FILE%"

echo.
echo === Setup Complete ===
echo Auto-start enabled. Client will auto-discover the server on LAN.
echo.
echo Commands:
echo   Stop:           taskkill /f /im python.exe
echo   Remove startup: del "%VBS_FILE%"
echo.
pause
