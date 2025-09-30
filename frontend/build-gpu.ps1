# GPU-accelerated build script for Meetily (Windows)
# Automatically detects and builds with optimal GPU features

Write-Host "🚀 Meetily GPU-Accelerated Build Script" -ForegroundColor Blue
Write-Host ""

# Function to check if command exists
function Test-CommandExists {
    param($command)
    $null = Get-Command $command -ErrorAction SilentlyContinue
    return $?
}

# Detect GPU
$features = ""

Write-Host "🔍 Detecting GPU capabilities..." -ForegroundColor Blue

# Check for NVIDIA GPU
if (Test-CommandExists "nvidia-smi") {
    Write-Host "✅ NVIDIA GPU detected" -ForegroundColor Green

    try {
        $gpuName = & nvidia-smi --query-gpu=name --format=csv,noheader | Select-Object -First 1
        Write-Host "   $gpuName" -ForegroundColor Green
    } catch {
        Write-Host "   Unable to query GPU name" -ForegroundColor Yellow
    }

    $features = "cuda"
    Write-Host "   Building with CUDA acceleration" -ForegroundColor Green

# Check for Vulkan support (AMD/Intel GPUs)
} elseif (Test-Path "C:\VulkanSDK" -Or (Test-CommandExists "vulkaninfo")) {
    Write-Host "⚠️  Vulkan detected (AMD/Intel GPU)" -ForegroundColor Yellow
    $features = "vulkan"
    Write-Host "   Building with Vulkan acceleration" -ForegroundColor Yellow

} else {
    Write-Host "⚠️  No GPU detected" -ForegroundColor Yellow
    Write-Host "   Building with CPU optimization (OpenBLAS)" -ForegroundColor Yellow
    $features = ""
}

# Change to build directory (we're already in frontend/)
if (Test-Path "src-tauri") {
    Set-Location "src-tauri"
} elseif (Test-Path "frontend\src-tauri") {
    Set-Location "frontend\src-tauri"
} else {
    Write-Host "❌ Error: Could not find src-tauri directory" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "📦 Building Meetily..." -ForegroundColor Blue

# Build command
try {
    if ($features -eq "") {
        cargo build --release
    } else {
        cargo build --release --features $features
    }

    if ($LASTEXITCODE -eq 0) {
        Write-Host ""
        Write-Host "✅ Build completed successfully!" -ForegroundColor Green
        Write-Host ""
        Write-Host "Build configuration:" -ForegroundColor Blue
        Write-Host "  OS: Windows"
        if ($features -eq "") {
            Write-Host "  Features: default (CPU optimized)"
        } else {
            Write-Host "  Features: $features"
        }
        Write-Host ""
        Write-Host "🎉 You can now run Meetily with GPU acceleration!" -ForegroundColor Green
    } else {
        throw "Build failed with exit code $LASTEXITCODE"
    }
} catch {
    Write-Host ""
    Write-Host "❌ Build failed: $_" -ForegroundColor Red
    exit 1
}