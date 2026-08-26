@echo off

echo Cleaning npm dependencies...
rd /s /q node_modules
del /f /q package-lock.json

echo Installing npm dependencies...
pnpm install

echo Building llama-helper sidecar...
cargo build --release -p llama-helper
if %ERRORLEVEL% neq 0 (
    echo ERROR: Failed to build llama-helper
    exit /b %ERRORLEVEL%
)

echo Copying llama-helper sidecar to binaries...
if not exist "src-tauri\binaries" mkdir "src-tauri\binaries"
copy /Y "..\target\release\llama-helper.exe" "src-tauri\binaries\llama-helper-x86_64-pc-windows-msvc.exe"
if %ERRORLEVEL% neq 0 (
    echo ERROR: Failed to copy llama-helper binary
    exit /b %ERRORLEVEL%
)

echo Building the project...
pnpm run tauri build
