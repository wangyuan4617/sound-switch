@echo off
setlocal
title sound-switch : uninstall autostart

set "PS1=%~dp0scripts\uninstall-autostart.ps1"

echo ============================================================
echo   sound-switch - uninstall "start at logon"
echo ============================================================
echo.

if not exist "%PS1%" (
    echo [ERROR] Script not found:
    echo         %PS1%
    echo.
    echo Please keep the package folder structure unchanged.
    echo.
    pause
    exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%PS1%" -Pause
if errorlevel 1 (
    echo.
    echo [ERROR] Uninstall failed. Please read the messages above.
    echo.
    pause
)

endlocal
