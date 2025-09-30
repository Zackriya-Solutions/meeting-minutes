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

REM Detect GPU capabilities
echo 🔍 Detecting GPU capabilities...

set "GPU_FEATURES="

REM Check for NVIDIA GPU
where nvidia-smi >nul 2>&1
if %errorlevel% equ 0 (
    echo ✅ NVIDIA GPU detected
    for /f "delims=" %%a in ('nvidia-smi --query-gpu^=name --format^=csv,noheader 2^>nul') do (
        echo    %%a
        goto :nvidia_detected
    )
    :nvidia_detected
    set "GPU_FEATURES=cuda"
    echo    Building with CUDA acceleration
) else (
    REM Check for Vulkan support (AMD/Intel GPUs)
    if exist "C:\VulkanSDK" (
        echo ⚠️  Vulkan SDK detected (AMD/Intel GPU)
        set "GPU_FEATURES=vulkan"
        echo    Building with Vulkan acceleration
    ) else (
        where vulkaninfo >nul 2>&1
        if !errorlevel! equ 0 (
            echo ⚠️  Vulkan detected (AMD/Intel GPU)
            set "GPU_FEATURES=vulkan"
            echo    Building with Vulkan acceleration
        ) else (
            echo ⚠️  No GPU detected
            echo    Building with CPU optimization (OpenBLAS)
        )
    )
)

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

REM Build based on GPU detection
cd src-tauri
if "%GPU_FEATURES%" == "" (
    echo    Building with default features (OpenBLAS CPU optimization)
    cargo build --release
) else (
    echo    Building with GPU features: %GPU_FEATURES%
    cargo build --release --features %GPU_FEATURES%
)

if errorlevel 1 (
    echo.
    echo ❌ Build failed
    cd ..
    exit /b 1
)

cd ..

echo.
echo ========================================
echo ✅ Build completed successfully!
echo ========================================
echo.
echo Build configuration:
echo   OS: Windows
if "%GPU_FEATURES%" == "" (
    echo   Features: default (CPU optimized with OpenBLAS)
) else (
    echo   Features: %GPU_FEATURES%
)
echo.
echo 🎉 You can now run Meetily with GPU acceleration!
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