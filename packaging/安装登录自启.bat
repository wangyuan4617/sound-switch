@echo off
setlocal
title sound-switch : install autostart

set "PS1=%~dp0scripts\install-autostart.ps1"

echo ============================================================
echo   sound-switch - install "start at logon" (background, no window)
echo ============================================================
echo.

if not exist "%PS1%" (
    echo [ERROR] Script not found:
    echo         %PS1%
    echo.
    echo Please keep the package folder structure unchanged,
    echo i.e. this .bat must stay next to sound-switch.exe and the scripts folder.
    echo.
    pause
    exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%PS1%" -StartNow -Pause
if errorlevel 1 (
    echo.
    echo [ERROR] Installation failed. Please read the messages above.
    echo.
    pause
)

endlocal
