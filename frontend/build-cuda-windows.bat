@echo off
REM ============================================================================
REM  Meetily - Windows release build with NVIDIA CUDA acceleration
REM ============================================================================
REM
REM  Why this script exists instead of `pnpm run tauri:build:cuda`:
REM
REM  1. Generator. The Visual Studio CMake generator aborts with "No CUDA toolset
REM     found" because CUDA's MSBuild integration is only installed into Visual
REM     Studio versions that existed when the CUDA toolkit was released. Ninja
REM     drives nvcc directly and needs no integration files - only cl.exe on
REM     PATH, which vcvars64 provides.
REM
REM  2. Architecture. ggml defaults to CUDA architectures 52;61;70;75. CUDA 13
REM     has dropped several of those, and none of them match a modern GPU.
REM     CUDAARCHS is CMake's environment default for CMAKE_CUDA_ARCHITECTURES.
REM     Check yours with:  nvidia-smi --query-gpu=compute_cap --format=csv
REM       89 = Ada (RTX 4000/5000 Ada), 86 = Ampere, 90 = Hopper
REM
REM  3. Host compiler. NVIDIA's support matrix lists MSVC 195x / Visual Studio
REM     2026 18.x as supported for native x86_64, but only from CUDA 13.1 on.
REM     Older toolkits reject it in host_config.h; this script waives the check
REM     for those and leaves it in place for toolkits that support the compiler.
REM
REM  4. C++ dialect. CUDA 13 ships CCCL, whose CUB headers require C++17, while
REM     nvcc still defaults to C++14 and ggml sets no CUDA standard.
REM
REM  5. Preprocessor. CCCL refuses MSVC's traditional preprocessor, so cl.exe is
REM     switched to the conforming one via -Xcompiler.
REM
REM  Measured on an RTX 5000 Ada with whisper large-v3: 0.1x real time on the
REM  CPU-only build, 7-10x with this one.
REM ============================================================================

REM Delayed expansion: the toolkit directory is discovered inside a loop, and paths here
REM contain spaces, so every assignment uses the set "VAR=value" form.
setlocal enabledelayedexpansion

set "VCVARS=C:\Program Files\Microsoft Visual Studio\18\Insiders\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VCVARS%" (
    echo ERROR: vcvars64.bat not found at "%VCVARS%"
    echo Edit VCVARS in this script to match your Visual Studio installation.
    exit /b 1
)

call "%VCVARS%" || exit /b 1

REM Prefer the newest installed toolkit; CUDA_PATH may still point at an older one.
set "CUDA_ROOT=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA"
set "CUDA_NEWEST="
for /f "delims=" %%D in ('dir /b /ad /o-n "%CUDA_ROOT%\v*" 2^>nul') do if not defined CUDA_NEWEST set "CUDA_NEWEST=%%D"

if defined CUDA_NEWEST (
    set "CUDA_PATH=%CUDA_ROOT%\!CUDA_NEWEST!"
    set "PATH=%CUDA_ROOT%\!CUDA_NEWEST!\bin;!PATH!"
    echo Using CUDA toolkit: !CUDA_NEWEST!
) else (
    echo ERROR: no CUDA toolkit found under "%CUDA_ROOT%"
    exit /b 1
)

if "%CUDAARCHS%"=="" set "CUDAARCHS=89"
set "CMAKE_GENERATOR=Ninja"
set "CUDAFLAGS=-std=c++17 -Xcompiler=/Zc:preprocessor"

REM Trailing backslash in %~dp0 would escape the closing quote, so anchor with a dot.
cd /d "%~dp0." || exit /b 1

echo.
echo === Building Meetily with CUDA (arch %CUDAARCHS%) ===
echo.

REM `call` is required: pnpm ships both an extensionless shell script and pnpm.cmd, and
REM without it cmd picks the wrong one and reports ERR_PNPM_RECURSIVE_EXEC_NO_PACKAGE.
REM
REM --bundles msi: the nsis target downloads its toolchain at build time, which fails
REM behind an authenticating proxy.
call pnpm exec tauri build --features cuda --bundles msi %*

exit /b %ERRORLEVEL%
