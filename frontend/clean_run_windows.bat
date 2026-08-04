@echo off
setlocal enabledelayedexpansion

REM Log level selector with default to INFO
set "LOG_LEVEL=%~1"
if "%LOG_LEVEL%"=="" set "LOG_LEVEL=info"

if /i "%LOG_LEVEL%"=="info" (
    set "RUST_LOG=%LOG_LEVEL%"
) else if /i "%LOG_LEVEL%"=="debug" (
    set "RUST_LOG=%LOG_LEVEL%"
) else if /i "%LOG_LEVEL%"=="trace" (
    set "RUST_LOG=%LOG_LEVEL%"
) else (
    echo Invalid log level: %LOG_LEVEL%. Valid options: info, debug, trace
    exit /b 1
)

echo Cleaning npm dependencies...
if exist node_modules rd /s /q node_modules
if exist .pnp.cjs del /f /q .pnp.cjs
if exist out rd /s /q out
if exist .next rd /s /q .next
REM ponytail: .next MUST go whenever node_modules is reinstalled, same
REM reasoning as clean_run.sh (fresh node_modules -> new webpack runtime IDs
REM -> stale .next chunks -> ChunkLoadError).

echo Installing dependencies...
call pnpm install
if errorlevel 1 exit /b 1

echo Building Next.js application...
call pnpm run build
if errorlevel 1 exit /b 1

echo Preparing llama-helper sidecar...
for /f "tokens=2" %%i in ('rustc -vV ^| findstr "host:"') do set TARGET_TRIPLE=%%i

call cargo build -p llama-helper --manifest-path ..\Cargo.toml
if errorlevel 1 exit /b 1

if not exist src-tauri\binaries mkdir src-tauri\binaries
copy /Y "..\target\debug\llama-helper.exe" "src-tauri\binaries\llama-helper-%TARGET_TRIPLE%.exe" >nul
if errorlevel 1 (
    echo Failed to copy llama-helper binary
    exit /b 1
)

echo Building Tauri app...

echo Stopping any previous Meetily/dev-server instance on port 3118...
for /f "tokens=5" %%a in ('netstat -aon ^| findstr :3118') do (
    taskkill /PID %%a /F >nul 2>&1
)

call pnpm run tauri dev
