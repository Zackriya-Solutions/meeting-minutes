<#
Builds Windows installers with CUDA acceleration.

By default, the installers are local development builds: Windows code signing
and Tauri updater signatures are disabled. Maintainers can run
`./build-cuda.ps1 -Signed` after configuring the repository's DigiCert and
Tauri updater signing environment.

Prerequisites:
- Rust MSVC toolchain
- Visual Studio C++ Build Tools
- CMake
- LLVM/libclang compatible with the repository's bindgen version
- NVIDIA CUDA Toolkit with Visual Studio integration
- Node.js with Corepack (preferred) or pnpm 9+

The script builds both the CUDA-enabled llama-helper sidecar and the Tauri
application.
#>

param([switch]$Signed)

$ErrorActionPreference = "Stop"

$frontendRoot = $PSScriptRoot
$repoRoot = Split-Path -Parent $frontendRoot
$targetRoot = Join-Path $repoRoot "target"
$temporaryToolsDirectory = Join-Path $targetRoot "build-cuda-tools"
$localTauriConfigPath = Join-Path $targetRoot "build-cuda-local.conf.json"
$originalPath = $env:PATH
$originalLibclangPath = $env:LIBCLANG_PATH

function Assert-Command {
    param([Parameter(Mandatory = $true)][string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found on PATH."
    }
}

function Assert-SigningEnvironment {
    Assert-Command "smctl"

    if (-not $env:DIGICERT_KEYPAIR_ALIAS) {
        throw "Signed builds require DIGICERT_KEYPAIR_ALIAS for Windows code signing."
    }

    if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
        throw "Signed builds require TAURI_SIGNING_PRIVATE_KEY for Tauri updater signatures."
    }
}

function Resolve-LibclangDirectory {
    if ($env:LIBCLANG_PATH) {
        $configuredLibrary = Join-Path $env:LIBCLANG_PATH "libclang.dll"
        if (Test-Path -LiteralPath $configuredLibrary) {
            return $env:LIBCLANG_PATH
        }
        throw "LIBCLANG_PATH is set to '$env:LIBCLANG_PATH', but libclang.dll was not found there."
    }

    $defaultDirectory = "C:\Program Files\LLVM\bin"
    if (Test-Path -LiteralPath (Join-Path $defaultDirectory "libclang.dll")) {
        return $defaultDirectory
    }

    throw "libclang.dll was not found. Install LLVM or set LIBCLANG_PATH to its bin directory."
}

function Assert-CudaVisualStudioIntegration {
    if (-not $env:CUDA_PATH -or -not (Test-Path -LiteralPath $env:CUDA_PATH)) {
        throw "CUDA_PATH is not set to an installed NVIDIA CUDA Toolkit."
    }

    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere)) {
        throw "vswhere.exe was not found. Install Visual Studio with the Desktop development with C++ workload."
    }

    $visualStudioPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $visualStudioPath) {
        throw "Visual Studio C++ Build Tools were not found."
    }

    $buildCustomizationsRoot = Join-Path $visualStudioPath "MSBuild\Microsoft\VC"
    $cudaVersion = (Split-Path -Leaf $env:CUDA_PATH) -replace '^v', ''
    $cudaTargets = Get-ChildItem -LiteralPath $buildCustomizationsRoot -Recurse -Filter "CUDA $cudaVersion.targets" -ErrorAction SilentlyContinue |
        Where-Object { $_.DirectoryName -like "*BuildCustomizations*" } |
        Select-Object -First 1

    if (-not $cudaTargets) {
        $integrationSource = Join-Path $env:CUDA_PATH "extras\visual_studio_integration\MSBuildExtensions"
        throw @"
CUDA $cudaVersion Visual Studio integration was not found under '$buildCustomizationsRoot'.
Repair the CUDA Toolkit installation after installing Visual Studio, or copy the
files from '$integrationSource' into the active VC BuildCustomizations directory.
"@
    }

    Write-Host "CUDA integration: $($cudaTargets.FullName)" -ForegroundColor DarkGray
}

function Initialize-PnpmCommand {
    $corepack = Get-Command corepack -ErrorAction SilentlyContinue
    if ($corepack) {
        New-Item -ItemType Directory -Force -Path $temporaryToolsDirectory | Out-Null
        $shimPath = Join-Path $temporaryToolsDirectory "pnpm.cmd"
        Set-Content -LiteralPath $shimPath -Encoding Ascii -Value "@echo off`r`n@corepack pnpm@10 %*"
        $env:PATH = "$temporaryToolsDirectory;$originalPath"
        return
    }

    $pnpm = Get-Command pnpm -ErrorAction SilentlyContinue
    if (-not $pnpm) {
        throw "Neither Corepack nor pnpm was found. Install Node.js with Corepack support."
    }

    $pnpmVersion = (& pnpm --version).Trim()
    if ($LASTEXITCODE -ne 0 -or [int]($pnpmVersion.Split('.')[0]) -lt 9) {
        throw "pnpm 9 or newer is required for frontend/pnpm-lock.yaml; found '$pnpmVersion'."
    }
}

function Invoke-Pnpm {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$PnpmArgs)

    & pnpm @PnpmArgs
    if ($LASTEXITCODE -ne 0) {
        throw "pnpm failed with exit code $LASTEXITCODE."
    }
}

New-Item -ItemType Directory -Force -Path $targetRoot | Out-Null

try {
    Assert-Command "cargo"
    Assert-Command "cmake"
    Assert-Command "node"
    Assert-Command "nvcc"

    $env:LIBCLANG_PATH = Resolve-LibclangDirectory
    Assert-CudaVisualStudioIntegration
    Initialize-PnpmCommand

    if ($Signed) {
        Assert-SigningEnvironment
        Write-Host "Signed build enabled; using the repository signing configuration." -ForegroundColor Yellow
    }
    else {
        Set-Content -LiteralPath $localTauriConfigPath -Encoding Ascii -Value '{"bundle":{"createUpdaterArtifacts":false}}'
        Write-Host "Unsigned local build; code signing and updater signatures are disabled." -ForegroundColor Yellow
    }

    Push-Location $repoRoot
    try {
        Write-Host "Building CUDA-enabled llama-helper..." -ForegroundColor Cyan
        cargo build --release -p llama-helper --features cuda
        if ($LASTEXITCODE -ne 0) {
            throw "CUDA llama-helper build failed with exit code $LASTEXITCODE."
        }

        $binaryDirectory = Join-Path $frontendRoot "src-tauri\binaries"
        $helperSource = Join-Path $targetRoot "release\llama-helper.exe"
        $helperDestination = Join-Path $binaryDirectory "llama-helper-x86_64-pc-windows-msvc.exe"
        New-Item -ItemType Directory -Force -Path $binaryDirectory | Out-Null
        Copy-Item -LiteralPath $helperSource -Destination $helperDestination -Force

        Push-Location $frontendRoot
        try {
            Write-Host "Installing frontend dependencies..." -ForegroundColor Cyan
            Invoke-Pnpm install --frozen-lockfile

            Write-Host "Building Meetily with CUDA..." -ForegroundColor Cyan
            $tauriCommand = Join-Path $frontendRoot "node_modules\.bin\tauri.CMD"
            $tauriArguments = @("build")
            if (-not $Signed) {
                $tauriArguments += @("--no-sign", "--config", $localTauriConfigPath)
            }
            $tauriArguments += @("--", "--features", "cuda")

            & $tauriCommand @tauriArguments
            if ($LASTEXITCODE -ne 0) {
                throw "Meetily CUDA build failed with exit code $LASTEXITCODE."
            }
        }
        finally {
            Pop-Location
        }

        Write-Host "CUDA build complete." -ForegroundColor Green
        Write-Host "Installers: $(Join-Path $targetRoot 'release\bundle')" -ForegroundColor Green
    }
    finally {
        Pop-Location
    }
}
finally {
    $env:PATH = $originalPath
    $env:LIBCLANG_PATH = $originalLibclangPath
    Remove-Item -LiteralPath $localTauriConfigPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $temporaryToolsDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
