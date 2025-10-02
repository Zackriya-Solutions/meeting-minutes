# GPU-accelerated build script for Meetily (Windows PowerShell)
# Automatically detects and builds with optimal GPU features

Write-Host "GPU-Accelerated Build Script for Meetily" -ForegroundColor Blue
Write-Host ""

# Function to check if command exists
function Test-CommandExists {
    param($command)
    $null = Get-Command $command -ErrorAction SilentlyContinue
    return $?
}

# Detect GPU
$features = ""

Write-Host "Detecting GPU capabilities..." -ForegroundColor Blue
Write-Host ""

# Check for NVIDIA GPU
if (Test-CommandExists "nvidia-smi") {
    Write-Host "[OK] NVIDIA GPU detected" -ForegroundColor Green

    try {
        $gpuName = & nvidia-smi --query-gpu=name --format=csv,noheader 2>$null | Select-Object -First 1
        if ($gpuName) {
            Write-Host "     GPU: $gpuName" -ForegroundColor Green
        }
    } catch {
        Write-Host "     Could not query GPU details" -ForegroundColor Yellow
    }

    # Check if CUDA prerequisites are available
    if ($env:CUDA_PATH) {
        $features = "cuda"
        Write-Host "     Building with CUDA acceleration" -ForegroundColor Green
    } else {
        Write-Host "     CUDA_PATH not set - falling back to CPU" -ForegroundColor Yellow
        Write-Host "     Install CUDA Toolkit to enable GPU acceleration" -ForegroundColor Yellow
        $features = ""
    }

# Check for Vulkan support (AMD/Intel GPUs)
} elseif ((Test-Path "C:\VulkanSDK") -or (Test-CommandExists "vulkaninfo")) {
    Write-Host "[INFO] Vulkan support detected" -ForegroundColor Cyan

    # Check if all required environment variables are set
    $vulkanSdkSet = $null -ne $env:VULKAN_SDK -and $env:VULKAN_SDK -ne ""
    $blasIncludeSet = $null -ne $env:BLAS_INCLUDE_DIRS -and $env:BLAS_INCLUDE_DIRS -ne ""

    if ($vulkanSdkSet -and $blasIncludeSet) {
        $features = "vulkan"
        Write-Host "       Building with Vulkan acceleration" -ForegroundColor Green
    } else {
        Write-Host "       Missing required environment variables:" -ForegroundColor Yellow
        if (-not $vulkanSdkSet) {
            Write-Host "       - VULKAN_SDK not set" -ForegroundColor Yellow
        }
        if (-not $blasIncludeSet) {
            Write-Host "       - BLAS_INCLUDE_DIRS not set" -ForegroundColor Yellow
        }
        Write-Host "       Falling back to CPU optimization (OpenBLAS)" -ForegroundColor Yellow
        Write-Host "" -ForegroundColor Yellow
        Write-Host "       To enable Vulkan acceleration:" -ForegroundColor Cyan
        Write-Host "       1. Install Vulkan SDK from https://vulkan.lunarg.com/" -ForegroundColor Cyan
        Write-Host "       2. Install OpenBLAS and set BLAS_INCLUDE_DIRS" -ForegroundColor Cyan
        $features = ""
    }

} else {
    Write-Host "[INFO] No GPU detected" -ForegroundColor Cyan
    Write-Host "       Building with CPU optimization (OpenBLAS)" -ForegroundColor Cyan
    $features = ""
}

Write-Host ""

# Change to build directory
$targetDir = ""
if (Test-Path "src-tauri\Cargo.toml") {
    $targetDir = "src-tauri"
} elseif (Test-Path "frontend\src-tauri\Cargo.toml") {
    $targetDir = "frontend\src-tauri"
} elseif (Test-Path "Cargo.toml") {
    $targetDir = "."
} else {
    Write-Host "[ERROR] Could not find Cargo.toml" -ForegroundColor Red
    Write-Host "        Make sure you're in the project root or frontend directory" -ForegroundColor Red
    exit 1
}

if ($targetDir -ne ".") {
    Write-Host "Changing to directory: $targetDir" -ForegroundColor Cyan
    Set-Location $targetDir
}

Write-Host ""
Write-Host "Building Meetily..." -ForegroundColor Blue
Write-Host ""

# Build command
$buildSuccess = $false

try {
    if ($features -eq "") {
        Write-Host "Running: cargo build --release" -ForegroundColor Cyan
        cargo build --release
    } else {
        Write-Host "Running: cargo build --release --features $features" -ForegroundColor Cyan
        cargo build --release --features $features
    }

    if ($LASTEXITCODE -eq 0) {
        $buildSuccess = $true
    }
} catch {
    Write-Host ""
    Write-Host "[ERROR] Build failed: $_" -ForegroundColor Red
    exit 1
}

if ($buildSuccess) {
    Write-Host ""
    Write-Host "======================================" -ForegroundColor Green
    Write-Host "Build completed successfully!" -ForegroundColor Green
    Write-Host "======================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Build configuration:" -ForegroundColor Cyan
    Write-Host "  OS: Windows"
    if ($features -eq "") {
        Write-Host "  Features: default (CPU with OpenBLAS)"
    } else {
        Write-Host "  Features: $features"
    }
    Write-Host ""
} else {
    Write-Host ""
    Write-Host "[ERROR] Build failed with exit code $LASTEXITCODE" -ForegroundColor Red
    exit 1
}