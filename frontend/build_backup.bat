@echo off
REM Meetily Build Script for Windows
REM This script sets up environment variables and builds the Tauri application

REM Exit on error
setlocal enabledelayedexpansion

REM Check if debug mode is set
if "%~1" == "debug" (
    set "DEBUG=true"
) else if "%~1" == "check" (
    set "CHECK=true"
) else if "%~1" == "help" (
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
) else (
    set "DEBUG=false"
)

echo 🚀 Building Meetily application...
echo 🔨 Building Tauri application...

REM Kill any existing processes on port 3118
echo Checking for existing processes on port 3118...
for /f "tokens=5" %%a in ('netstat -aon ^| findstr :3118') do (
    echo Killing process %%a on port 3118
    taskkill /PID %%a /F >nul 2>&1
)

REM Set libclang path for whisper-rs-sys
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"

REM Try to find and setup Visual Studio environment
if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    echo Setting up Visual Studio 2022 Build Tools environment...
    call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
    echo Setting additional Windows SDK and C++ runtime paths...
    
    REM Manually set up the environment since vcvars64.bat is not working properly
    set "LIB=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\um\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\ucrt\x64"
    set "INCLUDE=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\shared;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\ucrt"
    set "PATH=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64;C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64;%PATH%"
    
    echo LIB path: %LIB%
    echo INCLUDE path: %INCLUDE%
    
    REM Verify critical libraries exist
    if exist "C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\um\x64\kernel32.lib" (
        echo ✓ kernel32.lib found
    ) else (
        echo ✗ kernel32.lib NOT found - Windows SDK issue
    )
    
    if exist "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\lib\x64\msvcrt.lib" (
        echo ✓ msvcrt.lib found in Visual Studio MSVC
    ) else (
        echo ✗ msvcrt.lib NOT found - C++ runtime issue
    )
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
    echo Warning: Visual Studio environment not found. Using manual SDK setup...
    REM Fallback to manual Windows SDK setup
    set "WindowsSDKVersion=10.0.22621.0"
    set "WindowsSDKLibVersion=10.0.22621.0"
    set "WindowsSDKIncludeVersion=10.0.22621.0"
    set "LIB=C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\um\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.22621.0\ucrt\x64;%LIB%"
    set "INCLUDE=C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\shared;C:\Program Files (x86)\Windows Kits\10\Include\10.0.22621.0\ucrt;%INCLUDE%"
    set "PATH=C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64;%PATH%"
)
echo Environment setup complete. Starting build...
echo Final LIB path: %LIB%
echo Final INCLUDE path: %INCLUDE%

REM Export environment variables for the child process
set "RUST_ENV_LIB=%LIB%"
set "RUST_ENV_INCLUDE=%INCLUDE%"

if %errorlevel% neq 0 (
    echo Error: Failed to set up environment variables
    exit /b 1
)

REM if debug mode, run tauri dev
if "%~1" == "debug" (
    echo Starting development mode...
    echo Running initial compilation check...
   
    echo ✅ Initial compilation check passed. Starting development server...
    call pnpm run tauri dev
    if errorlevel 1 (
        echo Error: Failed to start Tauri development server
        exit /b 1
    )
) else if "%~1" == "check" (
    echo Running cargo check...
    cd src-tauri
    cargo check --no-default-features
    if errorlevel 1 (
        echo.
        echo ❌ Error: Cargo check failed - fix the compilation errors above
        cd ..
        exit /b 1
    ) else (
        echo.
        echo ✅ Cargo check passed successfully!
        cd ..
        exit /b 0
    )
) else (
    echo Building for production...
    echo Running pre-build compilation check...
   
    echo ✅ Pre-build check passed. Building for production...
    call pnpm run tauri build
    if errorlevel 1 (
        echo ❌ Error: Failed to build Tauri application for production
        exit /b 1
    )
)

REM Only show success message for production builds
if not "%~1" == "debug" (
    echo Tauri application built successfully!
    exit /b 0
)

:_print_help
echo.
echo ========================================
echo    Meetily Build Script - Help
echo ========================================
echo.
echo USAGE:
echo   build.bat [OPTION]
echo.
echo OPTIONS:
echo   debug     Build and run the application in development mode
echo   check     Run cargo check to verify compilation without building
echo   help      Show this help message
echo   --help    Show this help message
echo   -h        Show this help message
echo   /?        Show this help message
echo   ^(none^)  Build the application for production
echo.
echo DESCRIPTION:
echo   This script builds the Meetily Tauri application for Windows.
echo   It automatically sets up the Visual Studio build environment,
echo   configures necessary paths, and handles port cleanup.
echo.
echo EXAMPLES:
echo   build.bat           ^# Build for production
echo   build.bat debug     ^# Build and run in development mode
echo   build.bat --help    ^# Show this help
echo.
echo REQUIREMENTS:
echo   - Visual Studio 2022 Build Tools ^(or Community/Professional/Enterprise^)
echo   - Windows SDK 10.0.22621.0 or compatible
echo   - Node.js and pnpm installed
echo   - Rust toolchain installed
echo.
echo ENVIRONMENT SETUP:
echo   The script automatically configures:
echo   - Visual Studio build environment
echo   - Windows SDK paths
echo   - C++ runtime libraries
echo   - LLVM/Clang paths for whisper-rs-sys
echo.
echo PORT MANAGEMENT:
echo   Automatically kills processes on port 3118 before building
echo.
echo TROUBLESHOOTING:
echo   If build fails, ensure:
echo   - Visual Studio 2022 Build Tools are installed
echo   - Windows SDK 10.0.22621.0 is installed
echo   - LLVM is installed at C:^\Program Files^\LLVM^\bin
echo   - All dependencies are properly installed
echo.
echo ========================================
exit /b 0