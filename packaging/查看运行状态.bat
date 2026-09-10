@echo off
setlocal
title sound-switch : status

set "PS1=%~dp0scripts\status.ps1"

if not exist "%PS1%" (
    echo [ERROR] Script not found:
    echo         %PS1%
    echo.
    pause
    exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%PS1%" -Pause
if errorlevel 1 (
    echo.
    echo [ERROR] Failed to read status. Please read the messages above.
    echo.
    pause
)

endlocal
