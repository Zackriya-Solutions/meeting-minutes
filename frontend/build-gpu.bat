@echo off
REM Meetily GPU-Accelerated Build Script for Windows
REM Automatically detects and builds with optimal GPU features
REM Based on the existing build.bat with GPU detection enhancements

REM Exit on error
setlocal enabledelayedexpansion

REM Check if help is requested
if "%~1" == "help" (
    call :_print_help
    exit /b 0
) else if "%~1" == "--help" (
    call :_print_help
    exit /b 0
) else if "%~1" == "-h" (
    call :_print_help
    exit /b 0
) else if "%~1" == "/?" (
    call :_print_help
    exit /b 0
)

echo.
echo ========================================
echo   Meetily GPU-Accelerated Build
echo ========================================
echo.

echo.

REM Kill any existing processes on port 3118
echo 🧹 Checking for existing processes on port 3118...
for /f "tokens=5" %%a in ('netstat -aon ^| findstr :3118 2^>nul') do (
    echo    Killing process %%a on port 3118
    taskkill /PID %%a /F >nul 2>&1
)

REM Set libclang path for whisper-rs-sys
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"

REM Try to find and setup Visual Studio environment
echo 🔧 Setting up Visual Studio environment...
if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo    Using Visual Studio 2022 Build Tools
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1

    REM Manually set up the environment
    set "LIB=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\um\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\ucrt\x64"
    set "INCLUDE=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\shared;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\ucrt"
    set "PATH=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64;C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64;%PATH%"
) else if exist "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo    Using Visual Studio 2022 Build Tools
    call "C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" (
    echo    Using Visual Studio 2022 Community
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat" (
    echo    Using Visual Studio 2022 Professional
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat" (
    echo    Using Visual Studio 2022 Enterprise
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
) else if exist "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo    Using Visual Studio 2019 Build Tools
    call "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
) else (
    echo    ⚠️  Visual Studio not found, using manual SDK setup
    set "WindowsSDKVersion=10.0.22621.0"
    set "WindowsSDKLibVersion=10.0.22621.0"
    set "WindowsSDKIncludeVersion=10.0.22621.0"
    set "LIB=C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\um\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\ucrt\x64;%LIB%"
    set "INCLUDE=C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\shared;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\ucrt;%INCLUDE%"
    set "PATH=C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64;%PATH%"
)

REM Export environment variables for the child process
set "RUST_ENV_LIB=%LIB%"
set "RUST_ENV_INCLUDE=%INCLUDE%"

echo.
echo 📦 Building Meetily...
echo.

REM Find package.json location
if exist "package.json" (
    echo    Found package.json in current directory
) else if exist "frontend\package.json" (
    echo    Found package.json in frontend directory
    cd frontend
) else (
    echo    ❌ Error: Could not find package.json
    echo    Make sure you're in the project root or frontend directory
    exit /b 1
)

REM Check if pnpm or npm is available
where pnpm >nul 2>&1
if %errorlevel% equ 0 (
    set "USE_PNPM=1"
) else (
    set "USE_PNPM=0"
)

where npm >nul 2>&1
if %errorlevel% equ 0 (
    set "USE_NPM=1"
) else (
    set "USE_NPM=0"
)

if %USE_PNPM% equ 0 (
    if %USE_NPM% equ 0 (
        echo    ❌ Error: Neither npm nor pnpm found
        exit /b 1
    )
)

REM Build using npm scripts (which handle GPU detection automatically)
echo    Building complete Tauri application with automatic GPU detection...
echo.

if %USE_PNPM% equ 1 (
    pnpm run tauri:build:vulkan
) else (
    npm run tauri:build:vulkan
)

if errorlevel 1 (
    echo.
    echo ❌ Build failed
    exit /b 1
)

echo.
echo ========================================
echo ✅ Build completed successfully!
echo ========================================
echo.
echo 🎉 Complete Tauri application built with GPU acceleration!
echo.
exit /b 0

:_print_help
echo.
echo ========================================
echo   Meetily GPU Build Script - Help
echo ========================================
echo.
echo USAGE:
echo   build-gpu.bat [OPTION]
echo.
echo OPTIONS:
echo   help      Show this help message
echo   --help    Show this help message
echo   -h        Show this help message
echo   /?        Show this help message
echo.
echo DESCRIPTION:
echo   This script automatically detects your GPU and builds
echo   Meetily with optimal hardware acceleration features:
echo.
echo   - NVIDIA GPU    : Builds with CUDA acceleration
echo   - AMD/Intel GPU : Builds with Vulkan acceleration
echo   - No GPU        : Builds with OpenBLAS CPU optimization
echo.
echo REQUIREMENTS:
echo   - Visual Studio 2022 Build Tools
echo   - Windows SDK 10.0.22621.0 or compatible
echo   - Rust toolchain installed
echo   - LLVM installed at C:\Program Files\LLVM\bin
echo.
echo GPU REQUIREMENTS:
echo   CUDA:   NVIDIA GPU + CUDA Toolkit installed
echo   Vulkan: AMD/Intel GPU + Vulkan SDK installed
echo.
echo MANUAL GPU FEATURES:
echo   If you want to manually specify GPU features:
echo     cd src-tauri
echo     cargo build --release --features cuda
echo     cargo build --release --features vulkan
echo.
echo ========================================
exit /b 0