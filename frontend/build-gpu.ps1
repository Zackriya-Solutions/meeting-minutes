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

Write-Host ""

# Find package.json location
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
Write-Host "Building Meetily..." -ForegroundColor Blue
Write-Host ""

# Build command using npm scripts
$buildSuccess = $false

try {
    # Check if pnpm or npm is available
    $usePnpm = Test-CommandExists "pnpm"
    $useNpm = Test-CommandExists "npm"

    if (-not $usePnpm -and -not $useNpm) {
        Write-Host "[ERROR] Neither npm nor pnpm found" -ForegroundColor Red
        exit 1
    }

    Write-Host "Building complete Tauri application with automatic GPU detection..." -ForegroundColor Cyan
    Write-Host ""

    if ($usePnpm) {
        pnpm run tauri:build
    } else {
        npm run tauri:build
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
    Write-Host "Complete Tauri application built with GPU acceleration!" -ForegroundColor Green
    Write-Host ""
} else {
    Write-Host ""
    Write-Host "[ERROR] Build failed with exit code $LASTEXITCODE" -ForegroundColor Red
    exit 1
}