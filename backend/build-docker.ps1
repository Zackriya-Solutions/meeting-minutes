# Multi-platform Docker build script for Whisper Server and Meeting App
# Supports both CPU-only and GPU-enabled builds across multiple architectures
#
# WARNING: AUDIO PROCESSING WARNING:
# Docker containers with insufficient resources will drop audio chunks when
# the processing queue becomes full (MAX_AUDIO_QUEUE_SIZE=10, lib.rs:54).
# Ensure containers have adequate memory (8GB+) and CPU allocation.
# Monitor logs for 'Dropped old audio chunk' messages (lib.rs:330).

param(
    [Parameter(Position=0)]
    [ValidateSet('cpu', 'gpu', 'macos', 'both', 'test-gpu')]
    [string]$BuildType = 'cpu',
    
    [Alias('r')]
    [string]$Registry = $env:REGISTRY,
    
    [Alias('p')]
    [switch]$Push,
    
    [Alias('t')]
    [string]$Tag,
    
    [string]$Platforms,
    
    [string]$BuildArgs = $env:BUILD_ARGS,
    
    [switch]$NoCache,
    
    [switch]$DryRun,
    
    [Alias('h')]
    [switch]$Help
)

# Set error action preference
$ErrorActionPreference = 'Stop'

# Configuration
$ScriptDir = $PSScriptRoot
$WhisperProjectName = 'whisper-server'
$AppProjectName = 'meetily-backend'

# Platform detection for cross-platform compatibility
$DetectedOS = [System.Environment]::OSVersion.Platform
$IsWindows = $IsWindows -or ($env:OS -eq 'Windows_NT') -or ($env:ComSpec -like '*cmd.exe')
$IsLinux = $IsLinux -or ($DetectedOS -eq [System.PlatformID]::Unix -and (Test-Path '/proc/version'))
$IsMacOS = $IsMacOS -or ($DetectedOS -eq [System.PlatformID]::Unix -and -not (Test-Path '/proc/version'))

# Multi-platform Docker build script for Whisper Server and Meeting App

# Color functions - Move these to the top before any other code uses them
function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Error {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

function Handle-Error {
    param([string]$Message)
    Write-Error $Message
    exit 1
}

# ...existing code for param block and other variables...

# Platform detection can now use Write-Info safely
if ($IsMacOS) {
    Write-Info "macOS detected via PowerShell - will support macOS-optimized configurations"
} elseif ($IsWindows) {
    Write-Info "Windows detected - optimizing for Windows Docker Desktop"
}

# ...rest of existing code...s
# ...existing code...

# Platform detection - remove duplicate block and fix string interpolation
if ($IsMacOS) {
    Write-Info "macOS detected - will support macOS-optimized configurations"
} elseif ($IsWindows) {
    Write-Info "Windows detected - optimizing for Windows Docker Desktop"
}

# Default to current platform for local builds, multi-platform for registry pushes
if (-not $Platforms) {
    # For Windows builds, always use linux/amd64 unless explicitly overridden
    if ($IsWindows) {
        $Platforms = "linux/amd64"  # Changed from "gpu"
    } else {
        $arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq [System.Runtime.InteropServices.Architecture]::X64) { "amd64" } else { "arm64" }
        $Platforms = "linux/$arch"
    }
}

# Windows-specific GPU detection
function Test-WindowsGpuSupport {
    if (-not $IsWindows) {
        return $false
    }
    
    Write-Info 'Checking Windows GPU support...'
    
    # Check for NVIDIA GPU
    try {
        $nvidiaOutput = nvidia-smi --query-gpu=name --format=csv,noheader,nounits 2>$null
        if ($nvidiaOutput) {
            Write-Info 'NVIDIA GPU detected: $($nvidiaOutput -split '`n' | Select-Object -First 1)'
            
            # Check Docker GPU support
            try {
                Write-Info 'Testing Docker GPU support...'
                $dockerGpuTest = docker run --rm --gpus all nvidia/cuda:11.8-base-ubuntu20.04 nvidia-smi --query-gpu=name --format=csv,noheader,nounits 2>$null
                if ($dockerGpuTest) {
                    Write-Info '✓ Docker GPU support confirmed'
                    return $true
                } else {
                    Write-Warn '✗ Docker GPU support not available'
                    Write-Info '> Install nvidia-container-toolkit for GPU support'
                }
            } catch {
                Write-Warn '✗ Could not test Docker GPU support'
            }
        } else {
            Write-Info 'No NVIDIA GPU detected'
        }
    } catch {
        Write-Info 'NVIDIA drivers not installed or nvidia-smi not available'
    }
    
    return $false
}

function Show-Help {
    @'
Multi-platform Whisper Server and Meeting App Docker Builder

Usage: build-docker.ps1 [OPTIONS] [BUILD_TYPE]

BUILD_TYPE:
  cpu           Build whisper server CPU-only + meeting app (default)
  gpu           Build whisper server GPU-enabled + meeting app
  macos         Build whisper server macOS-optimized + meeting app (cross-platform compatibility)
  both          Build both whisper server versions + meeting app
  
OPTIONS:
  -Registry                 Docker registry (e.g. ghcr.io/user)
  -Push                     Push images to registry
  -Tag                      Custom tag (default: auto-generated)
  -Platforms PLATFORMS      Target platforms (default: current platform)
  -BuildArgs ARGS           Additional build arguments
  -NoCache                  Build without cache
  -DryRun                   Show commands without executing
  -Help                     Show this help

Examples:
  # Build whisper CPU version + meeting app for current platform
  .\build-docker.ps1 cpu
  
  # Build whisper GPU version + meeting app
  .\build-docker.ps1 gpu
  
  # Build whisper macOS-optimized version + meeting app
  .\build-docker.ps1 macos
  
  # Build both whisper versions + meeting app
  .\build-docker.ps1 both
  
  # Build GPU version for multiple platforms (requires -Push)
  .\build-docker.ps1 gpu -Platforms 'linux/amd64,linux/arm64' -Push
  
  # Build both versions and push to registry
  .\build-docker.ps1 both -Registry 'ghcr.io/myuser' -Push
  
  # Build with custom CUDA version
  .\build-docker.ps1 gpu -BuildArgs 'CUDA_VERSION=12.1.1'

Note: The meeting app is always built alongside the whisper server as they work as a package.

Environment Variables:
  REGISTRY      Docker registry prefix
  PUSH          Push to registry (true/false)
  PLATFORMS     Target platforms
  BUILD_ARGS    Additional build arguments
'@
}

# Function to check prerequisites
# Fix the prerequisites check
function Test-Prerequisites {
    Write-Info "Checking prerequisites..."
    
    # Check Docker
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        Handle-Error "Docker is not installed or not in PATH"
    }
    
    # Check Docker Buildx
    try {
        docker buildx version | Out-Null
    } catch {
        Handle-Error "Docker Buildx is not available. Please install Docker Desktop or enable Buildx"
    }
    
    # Check if buildx builder exists
    $builderExists = docker buildx ls | Select-String 'whisper-builder'
    if (-not $builderExists) {
        Write-Info "Creating multi-platform builder..."
        docker buildx create --name whisper-builder --platform $Platforms --use
    } else {
        Write-Info "Using existing whisper-builder"
        docker buildx use whisper-builder
    }
    
    Write-Info "Prerequisites check passed"
}

# Fix the whisper.cpp directory handling
Write-Info "Changing to whisper.cpp directory..."
try {
    Write-Info "Current directory: $ScriptDir"
    $whisperPath = Join-Path $ScriptDir "whisper.cpp"
    
    if (-not (Test-Path $whisperPath -PathType Container)) {
        Handle-Error "whisper.cpp directory not found at: $whisperPath"
    }
    
    Set-Location -Path $whisperPath
    Write-Info "Changed to directory: $(Get-Location)"
    
    # Check for custom server directory
    $customServerPath = Join-Path $ScriptDir "whisper-custom\server"
    Write-Info "Checking for custom server directory: $customServerPath"
    
    if (-not (Test-Path $customServerPath -PathType Container)) {
        Handle-Error "Directory not found: $customServerPath"
    }
    
    # Copy custom server files
    Write-Info "Copying custom server files..."
    try {
        Copy-Item -Path "$customServerPath\*" -Destination "examples\server\" -Recurse -Force
        Write-Info "Custom server files copied successfully"
    } catch {
        Handle-Error "Failed to copy custom server files: $_"
    }
} catch {
    Handle-Error "Failed to setup whisper.cpp directory: $_"
}

Write-Info 'Verifying server files...'
Get-ChildItem 'examples/server/' | Out-Null

Write-Info 'Returning to original directory...'
Set-Location $ScriptDir

# Function to generate image tag
# Fix the New-Tag function
function New-Tag {
    param(
        [string]$BuildType,
        [string]$CustomTag
    )
    
    if ($CustomTag) {
        return $CustomTag
    }
    
    $timestamp = Get-Date -Format 'yyyyMMdd'
    
    # Get git commit hash if available
    $gitHash = ''
    try {
        $gitHash = "-$(git rev-parse --short HEAD 2>$null)"
    } catch {
        # Git not available or not in repo
    }
    
    # Fix string interpolation
    switch ($BuildType) {
        'cpu' { return "cpu-$timestamp$gitHash" }
        'gpu' { return "gpu-$timestamp$gitHash" }
        'macos' { return "macos-$timestamp$gitHash" }
        'app' { return "app-$timestamp$gitHash" }
        default { return "$BuildType-$timestamp$gitHash" }
    }
}

# Fix the Build-Image function
function Build-Image {
    param(
        [string]$BuildType,
        [string]$Tag
    )
    
    # Store original directory
    $originalDir = Get-Location
    
    try {
        # Set project name based on build type
        $projectName = if ($BuildType -eq 'app') { $AppProjectName } else { $WhisperProjectName }
        
        # Determine Dockerfile path based on the actual directory structure
        $dockerfile = switch ($BuildType) {
            'app' { Join-Path $ScriptDir "Dockerfile.app" }
            'cpu' { Join-Path $ScriptDir "Dockerfile.server-cpu" }
            'gpu' { Join-Path $ScriptDir "Dockerfile.server-gpu" }
            'macos' { Join-Path $ScriptDir "Dockerfile.server-macos" }
            default { throw "Invalid build type: $BuildType" }
        }
        
        # Verify Dockerfile exists
        if (-not (Test-Path $dockerfile -PathType Leaf)) {
            throw "Dockerfile not found at: $dockerfile"
        }
        
        # Construct full tag
        $fullTag = if ($Registry) { 
            "$Registry/$projectName`:$Tag" 
        } else { 
            "$projectName`:$Tag" 
        }
        
        Write-Info "Building $BuildType image: $fullTag"
        Write-Info "Platforms: $Platforms"
        Write-Info "Dockerfile: $dockerfile"
        Write-Info "Build context: $ScriptDir"
        
        # Set working directory to backend root for build context
        Set-Location $ScriptDir
        
        # Build command array
        $buildCmd = @(
            'buildx'
            'build'
            '--platform'
            $Platforms
            '--file'
            $dockerfile
            '--tag'
            $fullTag
        )
        
        if ($Push) {
            $buildCmd += '--push'
        } else {
            $buildCmd += '--load'
        }
        
        if ($BuildArgs) {
            $buildCmd += '--build-arg'
            $buildCmd += $BuildArgs
        }
        
        $buildCmd += '.'
        
        # Convert array to space-separated string for display
        $cmdDisplay = $buildCmd -join ' '
        
        if ($DryRun) {
            Write-Info "DRY RUN - Command would be:"
            Write-Info "docker $cmdDisplay"
            return $true
        }
        
        try {
            Write-Info "Executing: docker $cmdDisplay"
            $process = Start-Process -FilePath 'docker' -ArgumentList $buildCmd -Wait -PassThru -NoNewWindow
            
            if ($process.ExitCode -eq 0) {
                Write-Info "Successfully built: $fullTag"
                
                # Also tag with generic tag for docker-compose compatibility
                if (-not $Push) {
                    $genericTag = if ($Registry) { 
                        "$Registry/$projectName`:$BuildType" 
                    } else { 
                        "$projectName`:$BuildType" 
                    }
                    
                    Write-Info "Tagging as generic: $genericTag"
                    docker tag $fullTag $genericTag
                    
                    # For meetily-backend, also tag as 'latest'
                    if ($BuildType -eq 'app') {
                        $latestTag = if ($Registry) { 
                            "$Registry/$projectName`:latest" 
                        } else { 
                            "$projectName`:latest" 
                        }
                        Write-Info "Tagging as latest: $latestTag"
                        docker tag $fullTag $latestTag
                    }
                }
                
                return $true
            } else {
                throw "Docker build failed with exit code: $($process.ExitCode)"
            }
        } catch {
            Write-Error "Failed to build: $fullTag"
            Write-Error $_.Exception.Message
            return $false
        }
    } finally {
        # Always return to original directory
        Set-Location $originalDir
    }
}

# Main function
function Main {
    Write-Info "=== Whisper Server Docker Builder ==="
    Write-Info "Build type: $BuildType"
    if ($Registry) {
        Write-Info "Registry: $Registry"
    } else {
        Write-Info "Registry: None"
    }
    Write-Info "Platforms: $Platforms"
    Write-Info "Push: $($Push.ToString())"

    # Windows-specific optimizations
    if ($IsWindows) {
        Write-Info 'Windows environment detected'
        
        # Auto-detect optimal build type if not explicitly set
        if ($BuildType -eq 'cpu' -and $PSBoundParameters.Count -eq 0) {
            $hasGpu = Test-WindowsGpuSupport
            if ($hasGpu) {
                Write-Info 'GPU support detected - consider using: .\build-docker.ps1 gpu'
                Write-Info 'Continuing with CPU build as requested'
            }
        } elseif ($BuildType -eq 'gpu') {
            $hasGpu = Test-WindowsGpuSupport
            if (-not $hasGpu) {
                Write-Warn 'GPU build requested but GPU support not available'
                Write-Info 'Building GPU image anyway (may fallback to CPU at runtime)'
            }
        }
    }
    
    # Auto-detect macOS and adjust build type if needed
    if ($IsMacOS -and $BuildType -eq 'cpu') {
        Write-Info 'macOS detected - switching from CPU to macOS-optimized build'
        $BuildType = 'macos'
    } elseif ($IsMacOS -and $BuildType -eq 'gpu') {
        Write-Warn 'GPU build requested on macOS - switching to macOS-optimized (CPU-only) build'
        $BuildType = 'macos'
    }
    
    # Check prerequisites
    Test-Prerequisites
    
    # Build images - always build meeting app alongside whisper server
    switch ($BuildType) {
        'cpu' {
            $whisperTag = New-Tag 'cpu' $Tag
            $appTag = New-Tag 'app' $Tag
            
            Write-Info 'Building whisper server (CPU) + meeting app...'
            $success1 = Build-Image 'cpu' $whisperTag
            $success2 = Build-Image 'app' $appTag
            
            if (-not ($success1 -and $success2)) {
                exit 1
            }
        }
        'gpu' {
            $whisperTag = New-Tag 'gpu' $Tag
            $appTag = New-Tag 'app' $Tag
            
            Write-Info 'Building whisper server (GPU) + meeting app...'
            $success1 = Build-Image 'gpu' $whisperTag
            $success2 = Build-Image 'app' $appTag
            
            if (-not ($success1 -and $success2)) {
                exit 1
            }
        }
        'macos' {
            $whisperTag = New-Tag 'macos' $Tag
            $appTag = New-Tag 'app' $Tag
            
            Write-Info 'Building whisper server (macOS-optimized) + meeting app...'
            $success1 = Build-Image 'macos' $whisperTag
            $success2 = Build-Image 'app' $appTag
            
            if (-not ($success1 -and $success2)) {
                exit 1
            }
        }
        'both' {
            $cpuTag = New-Tag 'cpu' $Tag
            $gpuTag = New-Tag 'gpu' $Tag
            $appTag = New-Tag 'app' $Tag
            
            Write-Info 'Building both whisper server versions + meeting app...'
            $success1 = Build-Image 'cpu' $cpuTag
            $success2 = Build-Image 'gpu' $gpuTag
            $success3 = Build-Image 'app' $appTag
            
            if (-not ($success1 -and $success2 -and $success3)) {
                exit 1
            }
        }
        'test-gpu' {
            Write-Info '=== GPU Support Test ==='
            if ($IsWindows) {
                Test-WindowsGpuSupport
            } else {
                Write-Info 'GPU test is currently Windows-specific'
            }
            exit 0
        }
        default {
            Handle-Error 'Invalid build type: $BuildType'
        }
    }
    
    Write-Info '=== Build Complete ==='
    
    # Show built images
    if (-not $DryRun -and -not $Push) {
        Write-Info 'Built images:'
        try {
            docker images $WhisperProjectName --format 'table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}'
            docker images $AppProjectName --format 'table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}'
        } catch {
            # Ignore errors if images command fails
        }
        
        # Automatically run containers after successful build
        Write-Info ''
        Write-Info '=== Starting Containers ==='
        Write-Info 'Launching whisper server and meeting app...'
        
        # Call run-docker.ps1 with appropriate build type
        $runScriptPath = Join-Path $ScriptDir 'run-docker.ps1'
        if (Test-Path $runScriptPath) {
            # Determine which GPU mode to use based on what was built
            $runArgs = @('start', '-d')  # Start in foreground mode
            # Note: NOT passing -d (detach) to keep containers in foreground
            # If GPU was built, use GPU mode
            if ($BuildType -eq 'gpu' -or $BuildType -eq 'both') {
                $runArgs += '-Gpu'
            } elseif ($BuildType -eq 'cpu') {
                $runArgs += '-Cpu'
            }
            # For 'macos' and 'app' types, let run-docker.ps1 auto-detect
            
            # Display comprehensive configuration that will be executed
            Write-Info ''
            Write-Info '=== Container Configuration Preview ==='
            Write-Info ''
            Write-Info '> Build Configuration:'
            Write-Info "   Script to execute: $runScriptPath"
            Write-Info "   Arguments: $($runArgs -join ' ')"
            Write-Info "   Build type: $BuildType"
            Write-Info "   Platforms: $Platforms"
            if ($Registry) {
                Write-Info "   Registry: $Registry"
            }
            if ($Tag) {
                Write-Info "   Custom tag: $Tag"
            }
            if ($BuildArgs) {
                Write-Info "   Build args: $BuildArgs"
            }
            Write-Info ''
            
            Write-Info ' Whisper Server Configuration:'
            # Get default model based on build type
            $defaultModel = switch ($BuildType) {
                'gpu' { 'models/ggml-base.en.bin' }
                'cpu' { 'models/ggml-base.en.bin' }
                'macos' { 'models/ggml-base.en.bin' }
                'both' { 'models/ggml-base.en.bin (CPU) / models/ggml-base.en.bin (GPU)' }
                default { 'models/ggml-base.en.bin' }
            }
            Write-Info "   Model: $(if ($env:WHISPER_MODEL) { $env:WHISPER_MODEL } else { $defaultModel })"
            Write-Info "   Host: $(if ($env:WHISPER_HOST) { $env:WHISPER_HOST } else { '0.0.0.0' })"
            Write-Info "   Port: $(if ($env:WHISPER_PORT) { $env:WHISPER_PORT } else { '8178' })"
            Write-Info "   Threads: $(if ($env:WHISPER_THREADS) { $env:WHISPER_THREADS } else { '0 (auto-detect)' })"
            Write-Info "   GPU Enabled: $(if ($env:WHISPER_USE_GPU) { $env:WHISPER_USE_GPU } else { 'true' })"
            Write-Info "   Language: $(if ($env:WHISPER_LANGUAGE) { $env:WHISPER_LANGUAGE } else { 'en' })"
            Write-Info "   Translation: $(if ($env:WHISPER_TRANSLATE) { $env:WHISPER_TRANSLATE } else { 'false' })"
            Write-Info "   Diarization: $(if ($env:WHISPER_DIARIZE) { $env:WHISPER_DIARIZE } else { 'false' })"
            Write-Info "   Show Progress: $(if ($env:WHISPER_PRINT_PROGRESS) { $env:WHISPER_PRINT_PROGRESS } else { 'true' })"
            Write-Info ''
            
            Write-Info '> Service Endpoints:'
            Write-Info "   Whisper Server: http://localhost:$(if ($env:WHISPER_PORT) { $env:WHISPER_PORT } else { '8178' })"
            Write-Info "   Meeting App: http://localhost:5167"
            Write-Info "   Health Check: http://localhost:$(if ($env:WHISPER_PORT) { $env:WHISPER_PORT } else { '8178' })/"
            Write-Info ''
            
            Write-Info '> Runtime Configuration:'
            Write-Info "   Container mode: $($BuildType.ToUpper())"
            Write-Info "   Resource allocation: 8GB+ memory recommended"
            Write-Info "   Audio queue size: 10 (MAX_AUDIO_QUEUE_SIZE)"
            if ($BuildType -eq 'gpu' -or $BuildType -eq 'both') {
                Write-Info "   GPU passthrough: Enabled (--gpus all)"
            }
            Write-Info ''
            
            # Check if user approves to run the config or they want to run it manually
            $response = Read-Host "Do you want to start the containers with this configuration? (Y/n)"
            if ($response -match "^(n|no)$") {
                Write-Info "Container startup cancelled by user."
                Write-Info "You can start them manually later with: .\run-docker.ps1 start"
                Write-Info ''
                Write-Info 'Available commands:'
                Write-Host '  Start containers interactive : .\run-docker.ps1 start -Interactive' -ForegroundColor Blue
                Write-Info "  Start with CPU:   .\run-docker.ps1 start -Cpu"

                
                if ($BuildType -eq 'gpu' -or $BuildType -eq 'both') {
                    Write-Info "  Start with GPU:   .\run-docker.ps1 start -Gpu"
                }
                Write-Info "  View logs:        .\run-docker.ps1 logs"
                Write-Info "  Check status:     .\run-docker.ps1 status"
                return
            }
            
            Write-Info "Starting containers..."
            Write-Info "Executing: .\run-docker.ps1 $($runArgs -join ' ')"
            & $runScriptPath @runArgs
            
            if ($LASTEXITCODE -eq 0) {
                Write-Info ''
                Write-Info ' Containers started successfully!'
                Write-Info ''
                Write-Info 'Service URLs:'
                Write-Info '  Whisper Server: http://localhost:8178'
                Write-Info '  Meeting App: http://localhost:5167'
                Write-Info ''
                Write-Info 'Commands:'
                Write-Info '  View logs:     .\run-docker.ps1 logs'
                Write-Info '  Check status:  .\run-docker.ps1 status'
                Write-Info '  Stop services: .\run-docker.ps1 stop'
            } else {
                Write-Warn 'Failed to start containers automatically'
                Write-Info 'You can start them manually with: .\run-docker.ps1 start'
            }
        } else {
            Write-Warn 'run-docker.ps1 not found - cannot auto-start containers'
        }
    }
}

# Execute main function
Main