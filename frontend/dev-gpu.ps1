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

# Run tauri dev using npm scripts (which handle GPU detection automatically)
try {
    # Check if pnpm or npm is available
    $usePnpm = Test-CommandExists "pnpm"
    $useNpm = Test-CommandExists "npm"

    if (-not $usePnpm -and -not $useNpm) {
        Write-Host "[ERROR] Neither npm nor pnpm found" -ForegroundColor Red
        exit 1
    }

    Write-Host "Starting complete Tauri application with automatic GPU detection..." -ForegroundColor Cyan
    Write-Host ""

    if ($usePnpm) {
        pnpm run tauri:dev
    } else {
        npm run tauri:dev
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