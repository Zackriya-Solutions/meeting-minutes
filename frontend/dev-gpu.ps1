# GPU-accelerated development script for Meetily (Windows PowerShell)
# Automatically detects and runs in development mode with optimal GPU features

Write-Host "GPU-Accelerated Development Mode for Meetily" -ForegroundColor Blue
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

# Find frontend directory with package.json
if (Test-Path "package.json") {
    Write-Host "Using current directory" -ForegroundColor Cyan
} elseif (Test-Path "frontend\package.json") {
    Write-Host "Changing to directory: frontend" -ForegroundColor Cyan
    Set-Location frontend
} else {
    Write-Host "[ERROR] Could not find package.json" -ForegroundColor Red
    Write-Host "        Make sure you're in the project root or frontend directory" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Starting Meetily in development mode..." -ForegroundColor Blue
Write-Host ""

# Run tauri dev using npm/pnpm
try {
    # Check if pnpm or npm is available
    $usePnpm = Test-CommandExists "pnpm"
    $useNpm = Test-CommandExists "npm"

    if (-not $usePnpm -and -not $useNpm) {
        Write-Host "[ERROR] Neither npm nor pnpm found" -ForegroundColor Red
        exit 1
    }

    if ($features -eq "") {
        Write-Host "Running: tauri dev" -ForegroundColor Cyan
        if ($usePnpm) {
            pnpm tauri dev
        } else {
            npm run tauri dev
        }
    } else {
        Write-Host "Running: tauri dev (features: $features)" -ForegroundColor Cyan
        # Set environment variable for cargo features
        $env:CARGO_BUILD_FEATURES = "--features $features"
        if ($usePnpm) {
            pnpm tauri dev -- -- --features $features
        } else {
            npm run tauri dev -- -- --features $features
        }
    }

    if ($LASTEXITCODE -eq 0) {
        Write-Host ""
        Write-Host "Development server stopped cleanly" -ForegroundColor Green
    } else {
        throw "Development server exited with code $LASTEXITCODE"
    }
} catch {
    Write-Host ""
    Write-Host "[ERROR] Development server failed: $_" -ForegroundColor Red
    exit 1
}