# Meet4Specs Build Script with Code Signing
# Loads signing credentials from .env file or environment variables
# Then calls build-gpu.bat to execute the build

Write-Host ""
Write-Host "========================================"
Write-Host "   Meet4Specs GPU Build (Signed)"
Write-Host "========================================"
Write-Host ""

# Try to load .env file if not already in environment (CI/CD)
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    if (Test-Path ".env") {
        Write-Host "📄 Loading environment variables from .env..."
        . "$PSScriptRoot\scripts\load-env.ps1"
        Load-EnvFile -EnvFilePath ".env" -Verbose
        Write-Host ""
    }
}

# Verify signing credentials are available
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    Write-Host "❌ Error: No signing credentials found" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please provide signing credentials:" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Method 1: Create .env file" -ForegroundColor Cyan
    Write-Host "  1. Copy .env.example to .env" -ForegroundColor White
    Write-Host "     cp .env.example .env" -ForegroundColor Gray
    Write-Host ""
    Write-Host "  2. Extract your signing key:" -ForegroundColor White
    Write-Host "     Get-Content .tauri\meet4specs.key -Raw" -ForegroundColor Gray
    Write-Host ""
    Write-Host "  3. Add to .env file:" -ForegroundColor White
    Write-Host "     TAURI_SIGNING_PRIVATE_KEY=<your-key-content>" -ForegroundColor Gray
    Write-Host "     TAURI_SIGNING_PRIVATE_KEY_PASSWORD=<your-password>" -ForegroundColor Gray
    Write-Host ""
    Write-Host "Method 2: Set environment variables directly (CI/CD)" -ForegroundColor Cyan
    Write-Host "     `$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content .tauri\meet4specs.key -Raw" -ForegroundColor Gray
    Write-Host "     `$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = 'your-password'" -ForegroundColor Gray
    Write-Host ""
    exit 1
}

# Confirm credentials loaded
Write-Host "✅ Signing key loaded successfully" -ForegroundColor Green
if ($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
    Write-Host "✅ Signing key password loaded" -ForegroundColor Green
}
Write-Host ""

# Call the main build-gpu.bat script
Write-Host "🚀 Starting build process..."
Write-Host ""

& ".\build-gpu.bat" $args

$buildExitCode = $LASTEXITCODE

# Clear the environment variables for security
$env:TAURI_SIGNING_PRIVATE_KEY = $null
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $null

if ($buildExitCode -eq 0) {
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "✅ Signed build completed successfully!" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Updater artifacts have been signed and are ready for release."
} else {
    Write-Host ""
    Write-Host "❌ Build failed with exit code: $buildExitCode" -ForegroundColor Red
}

exit $buildExitCode
