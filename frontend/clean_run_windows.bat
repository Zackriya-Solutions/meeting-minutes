@echo off
setlocal enabledelayedexpansion

REM ponytail: kill any stale Meet4Specs instance BEFORE doing anything else.
REM Meet4Specs uses tauri-plugin-single-instance and hides to the system tray on
REM window close instead of exiting, so a previous run's process can survive
REM indefinitely in the background. If left running, a fresh `tauri dev`
REM launch here just re-focuses that stale, tray-hidden window instead of
REM spawning a new one - and since this script also kills whatever is
REM listening on port 3118 (the Next.js dev server) below, that stale
REM window's webview loses its live connection and renders blank/white.
REM Killing it up front guarantees every run starts a genuinely fresh
REM instance connected to the dev server we are about to start.
echo Stopping any previous Meet4Specs instance...
taskkill /F /IM meet4specs.exe >nul 2>&1

REM Stop legacy Next.js servers before touching node_modules/.next. Current
REM Tauri dev uses its built-in static frontend server for ../out (see
REM tauri.conf.json) and no longer needs port 3118, but stopping an older
REM process prevents stale chunks or file locks from previous revisions.
echo Stopping any legacy frontend server on port 3118...
for /f "tokens=5" %%a in ('netstat -aon ^| findstr /r /c:":3118 .*LISTENING"') do (
    taskkill /PID %%a /F >nul 2>&1
)

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

REM `tauri dev` runs `pnpm run build` with wait=true through beforeDevCommand.
REM Once it succeeds, Tauri serves frontendDist (../out) internally. This is
REM deterministic desktop startup: no Next dev/HMR process, no 3118 race and
REM no remote devUrl bridge/capability ambiguity.

REM Set libclang path for whisper-rs-sys (bindgen needs this or it silently
REM mis-parses whisper.h into an opaque struct with a single `_address` field,
REM which is the root cause of "no field X on type whisper_full_params" errors).
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"

REM Try to find and setup Visual Studio environment (same fallback chain as build.bat)
if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Build Tools environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Build Tools environment...
    call "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Community environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Professional environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Enterprise environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2019 Build Tools environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
) else (
    echo Warning: Visual Studio environment not found. whisper-rs-sys build may fail.
)

REM Set libclang path for whisper-rs-sys (bindgen needs this or it silently
REM mis-parses whisper.h into an opaque struct with a single `_address` field,
REM which is the root cause of "no field X on type whisper_full_params" errors).
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"

REM Try to find and setup Visual Studio environment (same fallback chain as build.bat)
if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Build Tools environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Build Tools environment...
    call "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Community environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Professional environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Enterprise environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2019 Build Tools environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
) else (
    echo Warning: Visual Studio environment not found. whisper-rs-sys build may fail.
)

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

call pnpm run tauri dev
