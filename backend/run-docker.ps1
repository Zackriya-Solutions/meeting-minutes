# Easy deployment script for Whisper Server and Meeting App Docker containers
# Handles model downloads, GPU detection, and container management
#
# WARNING: AUDIO PROCESSING WARNING:
# Insufficient Docker resources cause audio drops! The audio processing system
# drops chunks when queue is full (MAX_AUDIO_QUEUE_SIZE=10, lib.rs:54).
# Symptoms: "Dropped old audio chunk" in logs (lib.rs:330-333).
# Solution: Allocate 8GB+ RAM and adequate CPU to Docker containers.

param(
    [Parameter(Position=0)]
    [ValidateSet("start", "stop", "restart", "logs", "status", "shell", "clean", "build", "models", "gpu-test", "setup-db", "compose", "help")]
    [string]$Command = "start",
    
    [Parameter(ValueFromRemainingArguments=$true)]
    [string[]]$RemainingArgs = @(),
    
    [switch]$DryRun,
    
    [Alias("h")]
    [switch]$Help,
    
    [Alias("i")]
    [switch]$Interactive
)

trap {
    Write-Host "CRASH DETECTED at line $($_.InvocationInfo.ScriptLineNumber)" -ForegroundColor Red
    Write-Host "Error: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "Stack: $($_.ScriptStackTrace)" -ForegroundColor Yellow
    Read-Host "Press Enter to continue or Ctrl+C to exit"
    exit 1
}
# Set error action preference
$ErrorActionPreference = "Stop"

# Configuration
$ScriptDir = $PSScriptRoot
$ComposeFile = Join-Path $ScriptDir "docker-compose.yml"
$WhisperProjectName = "whisper-server"
$WhisperContainerName = "whisper-server"
$AppProjectName = "meetily-backend"
$AppContainerName = "meetily-backend"
$DefaultPort = 8178
$DefaultAppPort = 5167
$DefaultModel = "base.en"
$PreferencesFile = Join-Path $ScriptDir ".docker-preferences"

# Available whisper models
$AvailableModels = @(
    "tiny", "tiny.en", "tiny-q5_1",
    "base", "base.en", "base-q5_1",
    "small", "small.en", "small-q5_1",
    "medium", "medium.en", "medium-q5_1",
    "large-v1", "large-v2", "large-v3",
    "large-v1-q5_1", "large-v2-q5_1", "large-v3-q5_1",
    "large-v1-turbo", "large-v2-turbo", "large-v3-turbo"
)

# Color functions
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

function Show-Help {
    @"
Whisper Server and Meeting App Docker Deployment Script

Usage: run-docker.ps1 [COMMAND] [OPTIONS]

COMMANDS:
  start         Start both whisper server and meeting app
  stop          Stop running services
  restart       Restart services
  logs          Show service logs (use -Service to specify, -Tail N, -NoFollow)
  status        Show service status
  shell         Open shell in running container (use -Service to specify)
  clean         Remove containers and images
  build         Build Docker images
  models        Manage whisper models
  gpu-test      Test GPU availability
  setup-db      Setup/migrate database from existing installation (standalone)
  compose       Pass commands directly to docker-compose

START OPTIONS:
  -Model, -m MODEL        Whisper model to use (default: base.en)
  -Port, -p PORT         Whisper port to expose (default: 8178)
  -AppPort PORT          Meeting app port to expose (default: 5167)
  -Gpu, -g               Force GPU mode for whisper
  -Cpu, -c               Force CPU mode for whisper
  -Language LANG         Language code (default: auto)
  -Translate             Enable translation to English
  # -Diarize               Enable speaker diarization (feature not available yet)
  -Detach, -d            Run in background
  -Interactive, -i       Interactive setup with prompts
  -EnvFile FILE          Load environment from file

BUILD OPTIONS:
  -BuildType TYPE        Build type: cpu, gpu, macos, both (default: cpu)
  -Registry REG          Docker registry for push
  -Push                  Push images to registry
  -Tag TAG               Custom tag for images

CLEAN OPTIONS:
  -Images                Also remove Docker images
  -Volumes               Also remove Docker volumes
  -All                   Remove everything (containers, images, volumes)
  -Force                 Skip confirmation prompts

MODELS OPTIONS:
  list                   List available models
  download MODEL         Download specific model
  remove MODEL          Remove downloaded model
  status                Show model storage status

EXAMPLES:
  # Start with defaults
  .\run-docker.ps1 start

  # Start with specific model and GPU
  .\run-docker.ps1 start -Model large-v3 -Gpu

  # Start interactively
  .\run-docker.ps1 start -Interactive

  # Build both CPU and GPU images
  .\run-docker.ps1 build -BuildType both

  # Show logs for whisper service
  .\run-docker.ps1 logs -Service whisper
  
  # Show last 50 lines without following
  .\run-docker.ps1 logs -Tail 50 -NoFollow

  # Clean everything
  .\run-docker.ps1 clean -All

  # Test GPU availability
  .\run-docker.ps1 gpu-test

Environment Variables:
  WHISPER_MODEL         Default model to use
  WHISPER_PORT          Default whisper port
  APP_PORT              Default app port
  DOCKER_REGISTRY       Default registry for builds
  FORCE_GPU            Force GPU mode (true/false)
  DEBUG                Enable debug output (true/false)
"@
}

# Global state tracking
$Global:SAVED_MODEL = $null
$Global:SAVED_PORT = $null
$Global:SAVED_APP_PORT = $null
$Global:SAVED_FORCE_MODE = $null
$Global:SAVED_LANGUAGE = $null
$Global:SAVED_TRANSLATE = $null
$Global:SAVED_DIARIZE = $null
$Global:SAVED_DB_SELECTION = $null

# GPU Detection Functions
function Get-GpuInfo {
    $gpuInfo = @{
        HasNvidia = $false
        HasAmd = $false
        HasIntel = $false
        NvidiaVersion = ""
        Devices = @()
        HasDockerGpu = $false
    }
    
    # Check for NVIDIA GPUs
    try {
        $nvidiaOutput = nvidia-smi --query-gpu=name,driver_version --format=csv,noheader,nounits 2>$null
        if ($nvidiaOutput -and $LASTEXITCODE -eq 0) {
            $gpuInfo.HasNvidia = $true
            $gpuInfo.Devices += $nvidiaOutput -split "`n" | Where-Object { $_.Trim() -ne "" }
            $firstLine = ($nvidiaOutput -split "`n")[0]
            if ($firstLine) {
                $gpuInfo.NvidiaVersion = ($firstLine -split ",")[-1].Trim()
            }
        }
    } catch {
        # nvidia-smi not available
    }
    
    # Check for Docker GPU support
    try {
        $dockerGpuTest = docker run --rm --gpus all nvidia/cuda:11.8-base-ubuntu20.04 nvidia-smi 2>$null
        if ($dockerGpuTest -and $LASTEXITCODE -eq 0) {
            $gpuInfo.HasDockerGpu = $true
        }
    } catch {
        # Docker GPU not available
    }
    
    return $gpuInfo
}

function Test-GpuAvailability {
    param([switch]$Silent)
    
    $gpuInfo = Get-GpuInfo
    
    if (-not $Silent) {
        Write-Info "=== GPU Detection Results ==="
        Write-Info "NVIDIA GPU: $(if ($gpuInfo.HasNvidia) { 'Available' } else { 'Not detected' })"
        if ($gpuInfo.HasNvidia) {
            Write-Info "NVIDIA Driver: $($gpuInfo.NvidiaVersion)"
            Write-Info "GPU Devices:"
            foreach ($device in $gpuInfo.Devices) {
                Write-Info "  - $device"
            }
        }
        Write-Info "Docker GPU Support: $(if ($gpuInfo.HasDockerGpu) { 'Available' } else { 'Not available' })"
    }
    
    return $gpuInfo.HasNvidia -and $gpuInfo.HasDockerGpu
}

# Docker Image Management
function Test-DockerImage {
    param([string]$ImageName)
    
    try {
        $result = docker images $ImageName --format "{{.Repository}}:{{.Tag}}" 2>$null
        return ($result -ne "" -and $LASTEXITCODE -eq 0)
    } catch {
        return $false
    }
}

function Get-DockerContainerStatus {
    param([string]$ContainerName)
    
    try {
        $status = docker ps -a --filter "name=$ContainerName" --format "{{.Status}}" 2>$null
        if ($LASTEXITCODE -ne 0) {
            return "error"
        }
        if ($status -and $status -match "Up") {
            return "running"
        } elseif ($status -and $status -match "Exited") {
            return "stopped"
        } else {
            return "not_found"
        }
    } catch {
        return "error"
    }
}

function Stop-DockerContainer {
    param([string]$ContainerName)
    
    $status = Get-DockerContainerStatus $ContainerName
    if ($status -eq "running") {
        Write-Info "Stopping container: $ContainerName"
        docker stop $ContainerName | Out-Null
        docker rm $ContainerName | Out-Null
        Write-Info "Container $ContainerName stopped and removed"
    } elseif ($status -eq "stopped") {
        Write-Info "Removing stopped container: $ContainerName"
        docker rm $ContainerName | Out-Null
    }
}

# Preferences Management
function Save-Preferences {
    param(
        [string]$Model,
        [int]$Port,
        [int]$AppPort,
        [string]$ForceMode,
        [string]$Language,
        [bool]$Translate,
        [bool]$Diarize,
        [string]$DbSelection
    )
    
    $preferences = @{
        Model = $Model
        Port = $Port
        AppPort = $AppPort
        ForceMode = $ForceMode
        Language = $Language
        Translate = $Translate
        Diarize = $Diarize
        DbSelection = $DbSelection
        Timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    }
    
    try {
        $preferences | ConvertTo-Json | Set-Content $PreferencesFile
        Write-Info "Preferences saved"
    } catch {
        Write-Warn "Failed to save preferences: $($_.Exception.Message)"
    }
}

function Load-Preferences {
    if (Test-Path $PreferencesFile) {
        try {
            $content = Get-Content $PreferencesFile -Raw -ErrorAction Stop
            if ($content -and $content.Trim()) {
                $preferences = $content | ConvertFrom-Json -ErrorAction Stop
                $Global:SAVED_MODEL = if ($preferences.PSObject.Properties['Model']) { $preferences.Model } else { $null }
                $Global:SAVED_PORT = if ($preferences.PSObject.Properties['Port']) { $preferences.Port } else { $null }
                $Global:SAVED_APP_PORT = if ($preferences.PSObject.Properties['AppPort']) { $preferences.AppPort } else { $null }
                $Global:SAVED_FORCE_MODE = if ($preferences.PSObject.Properties['ForceMode']) { $preferences.ForceMode } else { $null }
                $Global:SAVED_LANGUAGE = if ($preferences.PSObject.Properties['Language']) { $preferences.Language } else { $null }
                $Global:SAVED_TRANSLATE = if ($preferences.PSObject.Properties['Translate']) { $preferences.Translate } else { $null }
                # $Global:SAVED_DIARIZE = if ($preferences.PSObject.Properties['Diarize']) { $preferences.Diarize } else { $null }
                $Global:SAVED_DB_SELECTION = if ($preferences.PSObject.Properties['DbSelection']) { $preferences.DbSelection } else { $null }
                Write-Info "Loaded previous preferences from $($preferences.Timestamp)"
            }
        } catch {
            Write-Warn "Failed to load preferences: $($_.Exception.Message)"
            Write-Warn "Using default settings"
        }
    }
}

# Main command functions
function Invoke-StartCommand {
    # Parse arguments
    $model = if ($env:WHISPER_MODEL) { $env:WHISPER_MODEL } else { $DefaultModel }
    $port = if ($env:WHISPER_PORT) { [int]$env:WHISPER_PORT } else { $DefaultPort }
    $appPort = if ($env:APP_PORT) { [int]$env:APP_PORT } else { $DefaultAppPort }
    $forceMode = "auto"
    $detach = $false
    $envFile = ""
    $language = ""
    $translate = $false
    # $diarize = $false  # Feature not available yet
    
    # Parse remaining arguments
    for ($i = 0; $i -lt $RemainingArgs.Length; $i++) {
        switch ($RemainingArgs[$i]) {
            { $_ -in @("-Model", "-m") } {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $model = $RemainingArgs[$i + 1]
                    $i++
                }
            }
            { $_ -in @("-Port", "-p") } {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $port = [int]$RemainingArgs[$i + 1]
                    $i++
                }
            }
            "-AppPort" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $appPort = [int]$RemainingArgs[$i + 1]
                    $i++
                }
            }
            { $_ -in @("-Gpu", "-g") } {
                $forceMode = "gpu"
            }
            { $_ -in @("-Cpu", "-c") } {
                $forceMode = "cpu"
            }
            "-Language" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $language = $RemainingArgs[$i + 1]
                    $i++
                }
            }
            "-Translate" {
                $translate = $true
            }
            # "-Diarize" {  # Feature not available yet
            #     $diarize = $true
            # }
            { $_ -in @("-Detach", "-d") } {
                $detach = $true
            }
            "-EnvFile" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $envFile = $RemainingArgs[$i + 1]
                    $i++
                }
            }
        }
    }
    
    # Check if we should run interactive mode
    $runInteractive = $false
    $setupMode = "interactive"
    $hasSavedPreferences = $false
    
    # Try to load saved preferences
    if (Test-Path $PreferencesFile) {
        Load-Preferences
        if ($Global:SAVED_MODEL -or $Global:SAVED_PORT -or $Global:SAVED_APP_PORT) {
            $hasSavedPreferences = $true
        }
    }
    
    # Determine if we should run interactively
    if ($Interactive) {
        $runInteractive = $true
        if ($hasSavedPreferences) {
            # Show previous settings and ask user choice
            Write-Host "`n=== Previous Settings Found ===" -ForegroundColor Blue
            Write-Host "Your last configuration:" -ForegroundColor Green
            Write-Host "  Model: $(if ($Global:SAVED_MODEL) { $Global:SAVED_MODEL } else { $DefaultModel })"
            Write-Host "  Whisper Port: $(if ($Global:SAVED_PORT) { $Global:SAVED_PORT } else { $DefaultPort })"
            Write-Host "  App Port: $(if ($Global:SAVED_APP_PORT) { $Global:SAVED_APP_PORT } else { $DefaultAppPort })"
            Write-Host "  GPU Mode: $(if ($Global:SAVED_FORCE_MODE) { $Global:SAVED_FORCE_MODE } else { 'auto' })"
            Write-Host "  Language: $(if ($Global:SAVED_LANGUAGE) { $Global:SAVED_LANGUAGE } else { 'auto' })"
            Write-Host "  Translation: $(if ($Global:SAVED_TRANSLATE) { $Global:SAVED_TRANSLATE } else { 'false' })"
            Write-Host "  Diarization: $(if ($Global:SAVED_DIARIZE) { $Global:SAVED_DIARIZE } else { 'false' })"
            Write-Host "  Database: $(if ($Global:SAVED_DB_SELECTION) { $Global:SAVED_DB_SELECTION } else { 'fresh' })"
            Write-Host ""
            Write-Host "What would you like to do?"
            Write-Host "  1) Use previous settings"
            Write-Host "  2) Customize settings (interactive setup)"
            Write-Host "  3) Use defaults and skip interactive setup"
            Write-Host ""
            
            $choice = Read-Host "Choose option [default: 1]"
            if ([string]::IsNullOrWhiteSpace($choice)) { $choice = "1" }
            
            $setupMode = switch ($choice) {
                "1" { "previous" }
                "2" { "customize" }
                "3" { "defaults" }
                default { "previous" }
            }
        } else {
            $setupMode = "customize"
        }
    } elseif ($model -eq $DefaultModel -and -not $language) {
        # Auto-prompt if using defaults
        $runInteractive = $true
        if ($hasSavedPreferences) {
            # Show previous settings and ask user choice
            Write-Host "`n=== Previous Settings Found ===" -ForegroundColor Blue
            Write-Host "Your last configuration:" -ForegroundColor Green
            Write-Host "  Model: $(if ($Global:SAVED_MODEL) { $Global:SAVED_MODEL } else { $DefaultModel })"
            Write-Host "  Whisper Port: $(if ($Global:SAVED_PORT) { $Global:SAVED_PORT } else { $DefaultPort })"
            Write-Host "  App Port: $(if ($Global:SAVED_APP_PORT) { $Global:SAVED_APP_PORT } else { $DefaultAppPort })"
            Write-Host "  GPU Mode: $(if ($Global:SAVED_FORCE_MODE) { $Global:SAVED_FORCE_MODE } else { 'auto' })"
            Write-Host "  Language: $(if ($Global:SAVED_LANGUAGE) { $Global:SAVED_LANGUAGE } else { 'auto' })"
            Write-Host ""
            Write-Host "What would you like to do?"
            Write-Host "  1) Use previous settings"
            Write-Host "  2) Customize settings (interactive setup)"
            Write-Host "  3) Use defaults and skip interactive setup"
            Write-Host ""
            
            $choice = Read-Host "Choose option [default: 1]"
            if ([string]::IsNullOrWhiteSpace($choice)) { $choice = "1" }
            
            $setupMode = switch ($choice) {
                "1" { "previous" }
                "2" { "customize" }
                "3" { "defaults" }
                default { "previous" }
            }
        } else {
            $setupMode = "customize"
        }
    }
    
    # Interactive mode - prompt for settings
    if ($runInteractive) {
        $dbSelection = "fresh"
        $dbSetupNeeded = ""
        
        switch ($setupMode) {
            "previous" {
                # Use saved preferences
                Write-Host "`n=== Using Previous Settings ===" -ForegroundColor Green
                $model = if ($Global:SAVED_MODEL) { $Global:SAVED_MODEL } else { $model }
                $port = if ($Global:SAVED_PORT) { $Global:SAVED_PORT } else { $port }
                $appPort = if ($Global:SAVED_APP_PORT) { $Global:SAVED_APP_PORT } else { $appPort }
                $forceMode = if ($Global:SAVED_FORCE_MODE) { $Global:SAVED_FORCE_MODE } else { $forceMode }
                $language = if ($Global:SAVED_LANGUAGE) { $Global:SAVED_LANGUAGE } else { $language }
                $translate = if ($Global:SAVED_TRANSLATE -eq $true) { $true } else { $false }
                # $diarize = if ($Global:SAVED_DIARIZE -eq $true) { $true } else { $false }  # Feature not available yet
                $dbSelection = if ($Global:SAVED_DB_SELECTION) { $Global:SAVED_DB_SELECTION } else { "fresh" }
                
                Write-Info "Loaded previous configuration"
                Write-Host ""
            }
            "defaults" {
                # Use defaults, skip interactive setup
                Write-Host "`n=== Using Default Settings ===" -ForegroundColor Green
                Write-Info "Using default configuration"
                Write-Host ""
            }
            "customize" {
                # Full interactive setup with saved preferences as defaults
                Write-Host "`n=== Interactive Setup ===" -ForegroundColor Green
                Write-Host ""
                
                # Model selection - always show, using saved preference as default
                Write-Host "Model Selection" -ForegroundColor Blue -NoNewline
                Write-Host " " 
                $currentModel = if ($Global:SAVED_MODEL) { $Global:SAVED_MODEL } else { $model }
                Write-Info "Available models:"
                Write-Host ""
                for ($i = 0; $i -lt $AvailableModels.Length; $i++) {
                    $current = if ($AvailableModels[$i] -eq $currentModel) { " (current)" } else { "" }
                    Write-Host ("  {0,2}) {1}{2}" -f ($i + 1), $AvailableModels[$i], $current)
                }
                Write-Host ""
                Write-Host "Model size guide:" -ForegroundColor Yellow
                Write-Host "  tiny    (~39 MB)  - Fastest, least accurate"
                Write-Host "  base    (~142 MB) - Good balance of speed/accuracy"
                Write-Host "  small   (~244 MB) - Better accuracy"
                Write-Host "  medium  (~769 MB) - High accuracy"
                Write-Host "  large   (~1550 MB)- Best accuracy, slowest"
                Write-Host ""
                
                $modelChoice = Read-Host "Select model number (1-$($AvailableModels.Length)) or enter model name [default: $currentModel]"
                if ([string]::IsNullOrWhiteSpace($modelChoice)) {
                    $model = $currentModel
                } elseif ($modelChoice -match '^\d+$' -and [int]$modelChoice -ge 1 -and [int]$modelChoice -le $AvailableModels.Length) {
                    $model = $AvailableModels[[int]$modelChoice - 1]
                } elseif ($modelChoice -in $AvailableModels) {
                    $model = $modelChoice
                } else {
                    Write-Warn "Invalid selection, using default: $currentModel"
                    $model = $currentModel
                }
                Write-Host "Selected model: $model" -ForegroundColor Green
                Write-Host ""
                
                # Language selection
                Write-Host "Language Selection" -ForegroundColor Blue -NoNewline
                Write-Host " "
                $currentLanguage = if ($Global:SAVED_LANGUAGE) { $Global:SAVED_LANGUAGE } else { "auto" }
                Write-Info "Common languages:"
                Write-Host "  1) auto (automatic detection)$(if ($currentLanguage -eq 'auto') { ' (current)' })"
                Write-Host "  2) en (English)$(if ($currentLanguage -eq 'en') { ' (current)' })"
                Write-Host "  3) es (Spanish)$(if ($currentLanguage -eq 'es') { ' (current)' })"
                Write-Host "  4) fr (French)$(if ($currentLanguage -eq 'fr') { ' (current)' })"
                Write-Host "  5) de (German)$(if ($currentLanguage -eq 'de') { ' (current)' })"
                Write-Host "  6) it (Italian)$(if ($currentLanguage -eq 'it') { ' (current)' })"
                Write-Host "  7) pt (Portuguese)$(if ($currentLanguage -eq 'pt') { ' (current)' })"
                Write-Host "  8) ru (Russian)$(if ($currentLanguage -eq 'ru') { ' (current)' })"
                Write-Host "  9) ja (Japanese)$(if ($currentLanguage -eq 'ja') { ' (current)' })"
                Write-Host " 10) zh (Chinese)$(if ($currentLanguage -eq 'zh') { ' (current)' })"
                Write-Host " 11) Other (enter language code)"
                Write-Host ""
                
                $langChoice = Read-Host "Select language [default: $currentLanguage]"
                if ([string]::IsNullOrWhiteSpace($langChoice)) {
                    $language = $currentLanguage
                } else {
                    $language = switch ($langChoice) {
                        "1" { "auto" }
                        "2" { "en" }
                        "3" { "es" }
                        "4" { "fr" }
                        "5" { "de" }
                        "6" { "it" }
                        "7" { "pt" }
                        "8" { "ru" }
                        "9" { "ja" }
                        "10" { "zh" }
                        "11" { 
                            $customLang = Read-Host "Enter language code (e.g., ko, ar, hi)"
                            if ($customLang) { $customLang } else { $currentLanguage }
                        }
                        default { 
                            if ($langChoice -match '^[a-z]{2}$') { $langChoice } else { $currentLanguage }
                        }
                    }
                }
                Write-Host "Selected language: $language" -ForegroundColor Green
                Write-Host ""
                
                # Port configuration
                Write-Host "Whisper Server Port Selection" -ForegroundColor Blue -NoNewline
                Write-Host " "
                $currentPort = if ($Global:SAVED_PORT) { $Global:SAVED_PORT } else { $port }
                Write-Host "  Current: $currentPort"
                Write-Host "  Common alternatives: 8081, 8082, 8178, 9080"
                Write-Host ""
                $portInput = Read-Host "Enter Whisper server port [default: $currentPort]"
                if ([string]::IsNullOrWhiteSpace($portInput)) {
                    $port = $currentPort
                } elseif ($portInput -match '^\d+$' -and [int]$portInput -ge 1024 -and [int]$portInput -le 65535) {
                    $port = [int]$portInput
                } else {
                    Write-Warn "Invalid port, using default: $currentPort"
                    $port = $currentPort
                }
                Write-Host "Selected Whisper port: $port" -ForegroundColor Green
                Write-Host ""
                
                Write-Host "Meeting App Port Selection" -ForegroundColor Blue -NoNewline
                Write-Host " "
                $currentAppPort = if ($Global:SAVED_APP_PORT) { $Global:SAVED_APP_PORT } else { $appPort }
                Write-Host "  Current: $currentAppPort"
                Write-Host "  Common alternatives: 5168, 5169, 3000, 8000"
                Write-Host ""
                $appPortInput = Read-Host "Enter Meeting app port [default: $currentAppPort]"
                if ([string]::IsNullOrWhiteSpace($appPortInput)) {
                    $appPort = $currentAppPort
                } elseif ($appPortInput -match '^\d+$' -and [int]$appPortInput -ge 1024 -and [int]$appPortInput -le 65535) {
                    $appPort = [int]$appPortInput
                } else {
                    Write-Warn "Invalid port, using default: $currentAppPort"
                    $appPort = $currentAppPort
                }
                Write-Host "Selected Meeting app port: $appPort" -ForegroundColor Green
                Write-Host ""
                
                # Database setup selection
                Write-Host "Database Setup Selection" -ForegroundColor Blue -NoNewline
                Write-Host " "
                Write-Host "Database Setup Options:"
                Write-Host "1. fresh    - Start with fresh database"
                Write-Host "2. migrate  - Import from existing installation"
                $dbChoice = Read-Host "Choose database option (1-2) [default: 1]"
                $dbSelection = switch ($dbChoice) {
                    "2" { 
                        # Get database path from user
                        Write-Host ""
                        Write-Host "Database Migration Setup" -ForegroundColor Yellow
                        Write-Host "You can paste the full path to your existing database file below."
                        Write-Host "Example: C:\Users\YourName\AppData\Local\Meeting Minutes\meeting_minutes.db"
                        Write-Host ""
                        
                        do {
                            $dbPath = Read-Host "Paste or enter the full path to your existing database file"
                            
                            # Trim whitespace and remove quotes if user pasted a quoted path
                            if (-not [string]::IsNullOrWhiteSpace($dbPath)) {
                                $dbPath = $dbPath.Trim().Trim('"').Trim("'")
                            }
                            
                            if ([string]::IsNullOrWhiteSpace($dbPath)) {
                                Write-Warn "Please enter a valid path"
                                continue
                            }
                            
                            # Expand environment variables if present
                            $dbPath = [Environment]::ExpandEnvironmentVariables($dbPath)
                            
                            if (-not (Test-Path $dbPath)) {
                                Write-Warn "File not found: $dbPath"
                                Write-Host "Please check the path and try again." -ForegroundColor Yellow
                                continue
                            }
                            if (-not $dbPath.EndsWith(".db")) {
                                Write-Warn "Please select a .db file (the path should end with '.db')"
                                continue
                            }
                            
                            # Show file info for confirmation
                            $fileInfo = Get-Item $dbPath
                            $fileSizeKB = [math]::Round($fileInfo.Length / 1024, 2)
                            Write-Info "Database file found: $dbPath"
                            Write-Info "File size: $fileSizeKB KB, Last modified: $($fileInfo.LastWriteTime)"
                            
                            $dbPath # Return the path
                            break
                        } while ($true)
                    }
                    default { "fresh" }
                }
                Write-Host "Selected: $(if ($dbSelection -eq 'fresh') { 'Fresh database installation' } else { "Database migration from: $dbSelection" })" -ForegroundColor Green
                Write-Host ""
                
                # GPU configuration
                if ($forceMode -eq "auto") {
                    $gpuAvailable = Test-GpuAvailability -Silent
                    if ($gpuAvailable) {
                        Write-Host ""
                        $savedGpuMode = if ($Global:SAVED_FORCE_MODE) { $Global:SAVED_FORCE_MODE } else { "auto" }
                        $gpuDefault = if ($savedGpuMode -eq "cpu") { "n" } else { "Y" }
                        $gpuChoice = Read-Host "GPU detected. Use GPU acceleration? (Y/n) [current: $savedGpuMode]"
                        if ([string]::IsNullOrWhiteSpace($gpuChoice)) { $gpuChoice = $gpuDefault }
                        if ($gpuChoice -eq "n" -or $gpuChoice -eq "N") {
                            $forceMode = "cpu"
                        } else {
                            $forceMode = "gpu"
                        }
                    } else {
                        Write-Info "No GPU detected, using CPU mode"
                        $forceMode = "cpu"
                    }
                }
                
                # Advanced options
                Write-Host ""
                $savedTranslate = if ($Global:SAVED_TRANSLATE -eq $true) { "true" } else { "false" }
                $translateDefault = if ($savedTranslate -eq "true") { "y" } else { "N" }
                $translateChoice = Read-Host "Enable translation to English? (y/N) [current: $savedTranslate]"
                if ([string]::IsNullOrWhiteSpace($translateChoice)) { $translateChoice = $translateDefault }
                $translate = $translateChoice -eq "y" -or $translateChoice -eq "Y"
                
                # $savedDiarize = if ($Global:SAVED_DIARIZE -eq $true) { "true" } else { "false" }
                # $diarizeDefault = if ($savedDiarize -eq "true") { "y" } else { "N" }
                # $diarizeChoice = Read-Host "Enable speaker diarization? (y/N) [current: $savedDiarize]"
                # if ([string]::IsNullOrWhiteSpace($diarizeChoice)) { $diarizeChoice = $diarizeDefault }
                # $diarize = $diarizeChoice -eq "y" -or $diarizeChoice -eq "Y"
                
                # Save the new preferences
                # Save-Preferences -Model $model -Port $port -AppPort $appPort -ForceMode $forceMode -Language $language -Translate $translate -Diarize $diarize -DbSelection $dbSelection
                Save-Preferences -Model $model -Port $port -AppPort $appPort -ForceMode $forceMode -Language $language -Translate $translate -Diarize $false -DbSelection $dbSelection
                Write-Host ""
            }
        }
        
        # Handle database setup for all modes
        if ($dbSelection -ne "fresh" -and $dbSelection) {
            $dbSetupNeeded = $dbSelection
        }
    }
    
    # Use environment variables if set (only if not already customized via interactive setup)
    if (-not $runInteractive) {
        $model = if ($env:WHISPER_MODEL) { $env:WHISPER_MODEL } else { $model }
        $port = if ($env:WHISPER_PORT) { [int]$env:WHISPER_PORT } else { $port }
        $appPort = if ($env:APP_PORT) { [int]$env:APP_PORT } else { $appPort }
    }
    
    # Handle database setup if needed
    if ($dbSetupNeeded -and $dbSetupNeeded -ne "fresh") {
        Write-Info "Setting up database from selected source..."
        $dockerDbDir = Join-Path $ScriptDir "data"
        $dockerDbPath = Join-Path $dockerDbDir "meeting_minutes.db"
        
        # Create data directory
        if (-not (Test-Path $dockerDbDir)) {
            New-Item -ItemType Directory -Path $dockerDbDir -Force | Out-Null
        }
        
        # Copy the selected database
        try {
            Write-Info "Copying database from: $dbSetupNeeded"
            Copy-Item $dbSetupNeeded $dockerDbPath -Force
            Write-Info "Database copied successfully: $dockerDbPath"
            
            # Verify the copy worked
            if (Test-Path $dockerDbPath) {
                $sourceSize = (Get-Item $dbSetupNeeded).Length
                $targetSize = (Get-Item $dockerDbPath).Length
                if ($sourceSize -eq $targetSize) {
                    Write-Info "Database copy verified (size: $([math]::Round($targetSize/1024, 2)) KB)"
                } else {
                    Write-Warn "⚠ Database copy size mismatch - please verify manually"
                }
            } else {
                Write-Error " Database copy failed - file not found at destination"
                exit 1
            }
        } catch {
            Write-Error " Failed to copy database: $($_.Exception.Message)"
            exit 1
        }
    } elseif ($dbSelection -eq "fresh" -and $runInteractive) {
        Write-Info "Setting up fresh database..."
        $dockerDbDir = Join-Path $ScriptDir "data"
        $dockerDbPath = Join-Path $dockerDbDir "meeting_minutes.db"
        
        # Create data directory
        if (-not (Test-Path $dockerDbDir)) {
            New-Item -ItemType Directory -Path $dockerDbDir -Force | Out-Null
        }
        
        # Remove existing database if any
        if (Test-Path $dockerDbPath) {
            Remove-Item $dockerDbPath -Force
        }
        
        Write-Info "Fresh database setup complete"
    }
    
    # Check model availability and show download info
    Write-Info "Checking model availability: $model"
    $modelsDir = Join-Path $ScriptDir "models"
    # Extract just the model name from full path if provided
    $modelName = if ($model -match '^models/ggml-(.+)\.bin$') { $matches[1] } else { $model }
    $modelFile = Join-Path $modelsDir "ggml-$modelName.bin"
    
    if (Test-Path $modelFile) {
        $fileSize = (Get-Item $modelFile).Length / 1024 / 1024
        Write-Info "Model already available: $model ($([math]::Round($fileSize, 0)) MB)"
    } else {
        Write-Warn "Model not found locally: $model"
        
        # Show estimated download size
        switch -Wildcard ($modelName) {
            "tiny*" { Write-Info "Estimated download size: ~39 MB (fastest, least accurate)" }
            "base*" { Write-Info "Estimated download size: ~142 MB (good balance)" }
            "small*" { Write-Info "Estimated download size: ~244 MB (better accuracy)" }
            "medium*" { Write-Info "Estimated download size: ~769 MB (high accuracy)" }
            "large*" { Write-Info "Estimated download size: ~1550 MB (best accuracy)" }
        }
        
        # Write-Host ""
        # Write-Info "Model download options:"
        # Write-Info "   1. Download now (recommended for faster startup)"
        # Write-Info "   2. Auto-download in container (slower startup but automated)"
        # Write-Host ""
        
        $downloadChoice = "n"
        if ($downloadChoice -ne "n" -and $downloadChoice -ne "N") {
            Write-Info "Downloading model now..."
            # Set the remaining args for the models command
            $script:RemainingArgs = @("download", $modelName)
            Invoke-ModelsCommand
        } else {
            Write-Info "Model will be downloaded automatically in the container"
        }
    }
    
    # Validate model
    if ($modelName -notin $AvailableModels) {
        Write-Error "Invalid model: $modelName"
        Write-Info "Available models: $($AvailableModels -join ', ')"
        exit 1
    }
    
    # Stop existing containers
    Write-Info "Stopping existing containers..."
    Stop-DockerContainer $WhisperContainerName
    Stop-DockerContainer $AppContainerName
    
    # Determine GPU usage
    $useGpu = $false
    $dockerfile = "Dockerfile.server-cpu"
    $imageName = "${WhisperProjectName}:cpu"
    
    if ($forceMode -eq "gpu") {
        $gpuAvailable = Test-GpuAvailability -Silent
        if ($gpuAvailable) {
            $useGpu = $true
            $dockerfile = "Dockerfile.server-gpu"
            $imageName = "${WhisperProjectName}:gpu"
            Write-Info "Using GPU acceleration"
        } else {
            Write-Warn "GPU requested but not available, falling back to CPU"
        }
    } elseif ($forceMode -eq "auto") {
        $gpuAvailable = Test-GpuAvailability -Silent
        if ($gpuAvailable) {
            $useGpu = $true
            $dockerfile = "Dockerfile.server-gpu"
            $imageName = "${WhisperProjectName}:gpu"
            Write-Info "Auto-detected GPU, using GPU acceleration"
        } else {
            Write-Info "Auto-detected CPU mode"
        }
    } else {
        Write-Info "Using CPU mode"
    }
    
    # Check if images exist, build if necessary
    if (-not (Test-DockerImage $imageName)) {
        Write-Info "Image $imageName not found, building..."
        $buildArgs = @()
        if ($useGpu) {
            $buildArgs += "gpu"
        } else {
            $buildArgs += "cpu"
        }
        
        if ($DryRun) {
            Write-Info "DRY RUN - Would build: .\build-docker.ps1 $($buildArgs -join ' ')"
        } else {
            & ".\build-docker.ps1" @buildArgs
            if ($LASTEXITCODE -ne 0) {
                Write-Error "Failed to build Docker image"
                exit 1
            }
        }
    }
    
    # Check if app image exists
    $appImageName = "${AppProjectName}:app"
    if (-not (Test-DockerImage $appImageName)) {
        Write-Info "App image $appImageName not found, building..."
        if ($DryRun) {
            Write-Info "DRY RUN - Would build app image"
        } else {
            & ".\build-docker.ps1" "app"
            if ($LASTEXITCODE -ne 0) {
                Write-Error "Failed to build app Docker image"
                exit 1
            }
        }
    }
    
    # Prepare environment variables
    $env:WHISPER_MODEL = $model
    $env:MODEL_NAME = $model  # For docker-compose.yml compatibility
    $env:WHISPER_PORT = $port
    $env:APP_PORT = $appPort
    $env:WHISPER_LANGUAGE = $language
    $env:WHISPER_TRANSLATE = if ($translate) { "true" } else { "false" }
    # $env:WHISPER_DIARIZE = if ($diarize) { "true" } else { "false" }  # Feature not available yet
    
    # Set local models directory for volume mounting
    $modelsDir = Join-Path $ScriptDir "models"
    if (-not (Test-Path $modelsDir)) {
        New-Item -ItemType Directory -Path $modelsDir -Force | Out-Null
    }
    $env:LOCAL_MODELS_DIR = $modelsDir
    
    # Note: Database migration is handled earlier in the interactive setup process
    
    # Start containers
    Write-Info "Starting containers..."
    $composeArgs = @("docker-compose", "--profile", "default", "up")
    if ($detach) {
        $composeArgs += "-d"
    }
    
    # Add specific services
    $composeArgs += @("whisper-server", "meetily-backend")
    
    # Set appropriate dockerfile
    $env:DOCKERFILE = $dockerfile
    
    # Convert model name to proper path format for whisper.cpp
    $whisperModelPath = if ($model -match '^models/') {
        # Already in path format
        $model
    } else {
        # Convert model name to path format
        "models/ggml-$model.bin"
    }
    
    # Update environment variables with proper model path
    $env:WHISPER_MODEL = $whisperModelPath
    
    # Log configuration
    Write-Info "Starting Whisper Server + Meeting App..."
    Write-Info "Whisper Model: $whisperModelPath"
    Write-Info "Whisper Port: $port"
    Write-Info "Meeting App Port: $appPort"
    Write-Info "Docker mode: $dockerfile"
    
    if ($language) {
        Write-Info "Language: $language"
    }
    if ($translate) {
        Write-Info "Translation: enabled"
    }
    # if ($diarize) {  # Feature not available yet
    #     Write-Info "Diarization: enabled"
    # }
    
    if ($DryRun) {
        Write-Info "DRY RUN - Would run: $($composeArgs -join ' ')"
        Write-Info "Environment:"
        Write-Info "  WHISPER_MODEL=$whisperModelPath"
        Write-Info "  MODEL_NAME=$model"
        Write-Info "  WHISPER_PORT=$port"
        Write-Info "  APP_PORT=$appPort"
        Write-Info "  DOCKERFILE=$dockerfile"
        Write-Info "  WHISPER_LANGUAGE=$language"
        Write-Info "  WHISPER_TRANSLATE=$(if ($translate) { 'true' } else { 'false' })"
        # Write-Info "  WHISPER_DIARIZE=$(if ($diarize) { 'true' } else { 'false' })"  # Feature not available yet
    } else {
        if ($detach) {
            Write-Info "Starting services in background..."
            & docker-compose -f $ComposeFile --profile default up -d whisper-server meetily-backend
            
            if ($LASTEXITCODE -eq 0) {
                Write-Info "Services started in background"
                Write-Host ""
                Write-Info "Service URLs:"
                Write-Info "  Whisper Server: http://localhost:$port"
                Write-Info "  Meeting App: http://localhost:$appPort"
                Write-Host ""
                Write-Info "Useful commands:"
                Write-Info "  View logs:     .\run-docker.ps1 logs"
                Write-Info "  Check status:  .\run-docker.ps1 status"
                Write-Info "  Stop services: .\run-docker.ps1 stop"
                Write-Host ""
                
                # Check for model availability and wait for services to initialize
                Write-Info "Checking model availability and service initialization..."
                
                # Wait for model to be available
                $maxWait = 300  # 5 minutes max wait for model download
                $waitCount = 0
                $modelReady = $false
                $modelName = $model -replace '^.*/', '' -replace '^ggml-', '' -replace '.bin$', ''
                
                Write-Info "Waiting for model '$modelName' to be ready..."
                
                while ($waitCount -lt $maxWait) {
                    # Check if model file exists in container
                    try {
                        docker exec whisper-server test -s "/app/models/ggml-$modelName.bin" 2>$null | Out-Null
                        if ($LASTEXITCODE -eq 0) {
                            Write-Info "Model is ready: $modelName"
                            $modelReady = $true
                            break
                        }
                    } catch {
                        # Container might not be running yet
                    }
                    
                    # Show progress every 30 seconds
                    if (($waitCount % 30) -eq 0 -and $waitCount -gt 0) {
                        Write-Info "Still downloading model '$modelName'... $($waitCount)s elapsed"
                    }
                    
                    Start-Sleep -Seconds 5
                    $waitCount += 5
                }
                
                if (-not $modelReady) {
                    Write-Warn "Model download taking longer than expected. Check logs: .\run-docker.ps1 logs"
                }
                
                # Now wait for services to respond
                Write-Info "Waiting for services to respond..."
                $serviceWait = 60  # 1 minute for services to respond after model is ready
                $serviceCount = 0
                $whisperReady = $false
                $appReady = $false

                while ($serviceCount -lt $serviceWait) {
                    # Check if whisper server is responding
                    if (-not $whisperReady) {
                        try {
                            $response = Invoke-WebRequest -Uri "http://localhost:$port/" -TimeoutSec 3 -ErrorAction Stop
                            Write-Info "Whisper Server is responding"
                            $whisperReady = $true
                        } catch [System.Net.WebException] {
                            # Service not ready yet - this is expected
                        } catch [System.Net.Sockets.SocketException] {
                            # Connection refused - service not ready
                        } catch {
                            # Other errors - log but continue
                            Write-Warn "Whisper health check error: $($_.Exception.Message)"
                        }
                    }
                    
                    # Check if meeting app is responding  
                    if (-not $appReady) {
                        try {
                            $response = Invoke-WebRequest -Uri "http://localhost:$appPort/get-meetings" -TimeoutSec 3 -ErrorAction Stop
                            Write-Info "Meeting App is responding" 
                            $appReady = $true
                        } catch [System.Net.WebException] {
                            # Service not ready yet - this is expected
                        } catch [System.Net.Sockets.SocketException] {
                            # Connection refused - service not ready
                        } catch {
                            # Other errors - log but continue
                            Write-Warn "Meeting app health check error: $($_.Exception.Message)"
                        }
                    }
                    
                    # Both services ready
                    if ($whisperReady -and $appReady) {
                        Write-Info "All services are ready!"
                        break
                    }
                    
                    Start-Sleep -Seconds 3
                    $serviceCount += 3
                }
                
                # Final status check
                if (-not $whisperReady -and -not $appReady) {
                    Write-Warn "Services may still be starting up. Check logs: .\run-docker.ps1 logs"
                } elseif (-not $whisperReady) {
                    Write-Warn "Whisper Server not responding. Check logs: .\run-docker.ps1 logs -Service whisper"
                } elseif (-not $appReady) {
                    Write-Warn "Meeting App not responding. Check logs: .\run-docker.ps1 logs -Service app"
                }
            } else {
                Write-Error "Failed to start services"
                exit 1
            }
        } else {
            Write-Info "Starting services with live logs..."
            Write-Info "Press Ctrl+C to stop services"
            Write-Host ""
            
            & docker-compose -f $ComposeFile --profile default up whisper-server meetily-backend
            
            if ($LASTEXITCODE -eq 0) {
                Write-Info "Services stopped normally"
            } else {
                Write-Error "Services exited with error"
                exit 1
            }
        }
    }
}

function Invoke-StopCommand {
    Write-Info "Stopping services..."
    docker-compose -f $ComposeFile down
    Write-Info "Services stopped"
}

function Invoke-RestartCommand {
    Write-Info "Restarting services..."
    docker-compose -f $ComposeFile restart
    Write-Info "Services restarted"
}

function Invoke-LogsCommand {
    $service = ""
    $follow = $true
    $tail = ""
    
    # Parse arguments
    for ($i = 0; $i -lt $RemainingArgs.Length; $i++) {
        switch ($RemainingArgs[$i]) {
            "-Service" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $service = $RemainingArgs[$i + 1]
                    $i++
                }
            }
            "-NoFollow" {
                $follow = $false
            }
            "-Tail" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $tail = $RemainingArgs[$i + 1]
                    $i++
                }
            }
        }
    }
    
    # Map service aliases to actual service names
    $serviceMap = @{
        "whisper" = "whisper-server"
        "app" = "meetily-backend"
        "backend" = "meetily-backend"
        "meeting" = "meetily-backend"
    }
    
    # Resolve service name if alias was used
    if ($service -and $serviceMap.ContainsKey($service)) {
        $service = $serviceMap[$service]
    }
    
    # Build docker-compose logs command
    $logArgs = @("logs")
    if ($follow) {
        $logArgs += "-f"
    }
    if ($tail) {
        $logArgs += @("--tail", $tail)
    }
    
    # Add service name(s) or show all
    if ($service) {
        # Validate service name
        $validServices = @("whisper-server", "meetily-backend")
        if ($service -notin $validServices) {
            Write-Warn "Unknown service: $service"
            Write-Info "Available services: whisper-server, meetily-backend"
            Write-Info "Aliases: whisper, app, backend, meeting"
            return
        }
        $logArgs += $service
    } else {
        # Show logs for both services
        $logArgs += @("whisper-server", "meetily-backend")
    }
    
    # Execute docker-compose logs
    docker-compose -f $ComposeFile @logArgs
}

function Invoke-StatusCommand {
    Write-Info "=== Service Status ==="
    docker-compose -f $ComposeFile ps
    
    Write-Info ""
    Write-Info "=== Container Details ==="
    $whisperStatus = Get-DockerContainerStatus $WhisperContainerName
    $appStatus = Get-DockerContainerStatus $AppContainerName
    
    Write-Info "Whisper Server: $whisperStatus"
    Write-Info "Meeting App: $appStatus"
    
    # Get actual ports from running containers
    if ($whisperStatus -eq "running") {
        try {
            $whisperPort = docker port $WhisperContainerName 8178 2>$null | ForEach-Object { $_ -replace '.*:', '' } | Select-Object -First 1
            if ($whisperPort) {
                Write-Info "Whisper Server URL: http://localhost:$whisperPort"
            } else {
                Write-Info "Whisper Server URL: http://localhost:$DefaultPort"
            }
        } catch {
            Write-Info "Whisper Server URL: http://localhost:$DefaultPort"
        }
    }
    
    if ($appStatus -eq "running") {
        try {
            $appPort = docker port $AppContainerName 5167 2>$null | ForEach-Object { $_ -replace '.*:', '' } | Select-Object -First 1
            if ($appPort) {
                Write-Info "Meeting App URL: http://localhost:$appPort"
            } else {
                Write-Info "Meeting App URL: http://localhost:$DefaultAppPort"
            }
        } catch {
            Write-Info "Meeting App URL: http://localhost:$DefaultAppPort"
        }
    }
}

function Invoke-ShellCommand {
    $service = "whisper"
    for ($i = 0; $i -lt $RemainingArgs.Length; $i++) {
        if ($RemainingArgs[$i] -eq "-Service" -and $i + 1 -lt $RemainingArgs.Length) {
            $service = $RemainingArgs[$i + 1]
            break
        }
    }
    
    Write-Info "Opening shell in $service container..."
    docker-compose -f $ComposeFile exec $service /bin/bash
}

function Invoke-CleanCommand {
    $removeImages = $false
    $removeVolumes = $false
    $removeAll = $false
    $force = $false
    
    for ($i = 0; $i -lt $RemainingArgs.Length; $i++) {
        switch ($RemainingArgs[$i]) {
            "-Images" { $removeImages = $true }
            "-Volumes" { $removeVolumes = $true }
            "-All" { $removeAll = $true }
            "-Force" { $force = $true }
        }
    }
    
    if ($removeAll) {
        $removeImages = $true
        $removeVolumes = $true
    }
    
    if (-not $force) {
        Write-Warn "This will remove Docker containers$(if ($removeImages) { ', images' })$(if ($removeVolumes) { ', volumes' })"
        $confirm = Read-Host "Are you sure? (y/N)"
        if ($confirm -ne "y" -and $confirm -ne "Y") {
            Write-Info "Cancelled"
            return
        }
    }
    
    Write-Info "Cleaning up Docker resources..."
    
    # Stop and remove containers
    docker-compose -f $ComposeFile down
    
    if ($removeVolumes) {
        Write-Info "Removing volumes..."
        docker-compose -f $ComposeFile down -v
    }
    
    if ($removeImages) {
        Write-Info "Removing images..."
        $images = @($WhisperProjectName, $AppProjectName)
        foreach ($image in $images) {
            try {
                $imageExists = docker images $image --format "{{.Repository}}" 2>$null
                if ($imageExists -and $LASTEXITCODE -eq 0) {
                    docker rmi $(docker images $image -q) 2>$null
                    Write-Info "Removed images for: $image"
                }
            } catch {
                # Image doesn't exist or error removing
            }
        }
    }
    
    Write-Info "Cleanup completed"
}

function Invoke-BuildCommand {
    $buildType = "cpu"
    $registry = ""
    $push = $false
    $tag = ""
    
    for ($i = 0; $i -lt $RemainingArgs.Length; $i++) {
        switch ($RemainingArgs[$i]) {
            "-BuildType" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $buildType = $RemainingArgs[$i + 1]
                    $i++
                }
            }
            "-Registry" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $registry = $RemainingArgs[$i + 1]
                    $i++
                }
            }
            "-Push" {
                $push = $true
            }
            "-Tag" {
                if ($i + 1 -lt $RemainingArgs.Length) {
                    $tag = $RemainingArgs[$i + 1]
                    $i++
                }
            }
        }
    }
    
    $buildArgs = @($buildType)
    if ($registry) { $buildArgs += @("-Registry", $registry) }
    if ($push) { $buildArgs += "-Push" }
    if ($tag) { $buildArgs += @("-Tag", $tag) }
    
    Write-Info "Building Docker images..."
    Write-Info "Command: .\build-docker.ps1 $($buildArgs -join ' ')"
    
    if ($DryRun) {
        Write-Info "DRY RUN - Would execute build command"
    } else {
        & ".\build-docker.ps1" @buildArgs
    }
}

function Invoke-ModelsCommand {
    if ($RemainingArgs.Length -eq 0) {
        Write-Info "Models command requires a subcommand: list, download, remove, status"
        return
    }
    
    $subCommand = $RemainingArgs[0]
    
    switch ($subCommand) {
        "list" {
            Write-Info "=== Available Whisper Models ==="
            foreach ($model in $AvailableModels) {
                Write-Info "  $model"
            }
        }
        "download" {
            if ($RemainingArgs.Length -lt 2) {
                Write-Error "download command requires a model name"
                return
            }
            $modelName = $RemainingArgs[1]
            if ($modelName -notin $AvailableModels) {
                Write-Error "Invalid model: $modelName"
                Write-Info "Available models: $($AvailableModels -join ', ')"
                return
            }
            
            # Ensure models directory exists
            $modelsDir = Join-Path $ScriptDir "models"
            if (-not (Test-Path $modelsDir)) {
                New-Item -ItemType Directory -Path $modelsDir -Force | Out-Null
            }
            
            $modelFile = Join-Path $modelsDir "ggml-$modelName.bin"
            
            if (Test-Path $modelFile) {
                $fileSize = (Get-Item $modelFile).Length / 1024 / 1024
                Write-Info "Model already exists: $modelFile ($([math]::Round($fileSize, 0)) MB)"
                return
            }
            
            # Show download information
            Write-Info "Downloading model: $modelName"
            switch -Wildcard ($modelName) {
                "tiny*" { Write-Info "Size: ~39 MB (fastest, least accurate)" }
                "base*" { Write-Info "Size: ~142 MB (good balance)" }
                "small*" { Write-Info "Size: ~244 MB (better accuracy)" }
                "medium*" { Write-Info "Size: ~769 MB (high accuracy)" }
                "large*" { Write-Info "Size: ~1550 MB (best accuracy)" }
            }
            
            $downloadUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-$modelName.bin"
            Write-Info "URL: $downloadUrl"
            
            # Create a temporary file for download
            $tempFile = "$modelFile.tmp"
            
            # Download with progress and error handling
            Write-Info "Starting download..."
            try {
                # Use Invoke-WebRequest with proper parameter escaping
                $progressPreference = $ProgressPreference
                $ProgressPreference = 'SilentlyContinue'
                Invoke-WebRequest -Uri "$downloadUrl" -OutFile "$tempFile" -UseBasicParsing -ErrorAction Stop
                $ProgressPreference = $progressPreference
                
                # Verify download completed successfully
                if (Test-Path $tempFile -and (Get-Item $tempFile).Length -gt 0) {
                    Move-Item $tempFile $modelFile
                    $fileSize = (Get-Item $modelFile).Length / 1024 / 1024
                    Write-Info "Model downloaded successfully: $modelFile ($([math]::Round($fileSize, 0)) MB)"
                } else {
                    Write-Error "Downloaded file is empty"
                    if (Test-Path $tempFile) { Remove-Item $tempFile }
                    return
                }
            } catch {
                Write-Error "Failed to download model from $downloadUrl"
                Write-Error "Error: $($_.Exception.Message)"
                if (Test-Path $tempFile) { Remove-Item $tempFile }
                return
            } finally {
                # Cleanup temporary file if it exists
                if (Test-Path $tempFile) {
                    try { Remove-Item $tempFile -Force -ErrorAction SilentlyContinue } catch { }
                }
            }
        }
        "remove" {
            if ($RemainingArgs.Length -lt 2) {
                Write-Error "remove command requires a model name"
                return
            }
            $modelName = $RemainingArgs[1]
            Write-Info "Removing model: $modelName"
            # Implementation would remove model files
            $modelFile = Join-Path $ScriptDir "models" "ggml-$modelName.bin"
            if (Test-Path $modelFile) {
                Remove-Item $modelFile -Force
                Write-Info "Model removed: $modelFile"
            } else {
                Write-Warn "Model not found: $modelFile"
            }
        }
        "status" {
            Write-Info "=== Model Storage Status ==="
            try {
                docker run --rm -v whisper-models:/models alpine ls -la /models
            } catch {
                Write-Warn "Unable to check Docker volume status"
            }
        }
        default {
            Write-Error "Unknown models subcommand: $subCommand"
            Write-Info "Available subcommands: list, download, remove, status"
        }
    }
}

function Invoke-GpuTestCommand {
    Write-Info "=== GPU Test ==="
    $gpuAvailable = Test-GpuAvailability
    
    if ($gpuAvailable) {
        Write-Info "GPU is available and ready for use!"
        
        # Test Docker GPU access
        Write-Info "Testing Docker GPU access..."
        try {
            $result = docker run --rm --gpus all nvidia/cuda:11.8-base-ubuntu20.04 nvidia-smi
            Write-Info "Docker GPU test successful!"
            Write-Info $result
        } catch {
            Write-Error "Docker GPU test failed: $($_.Exception.Message)"
        }
    } else {
        Write-Warn "GPU is not available"
        Write-Info "Possible reasons:"
        Write-Info "  - No NVIDIA GPU installed"
        Write-Info "  - NVIDIA drivers not installed"
        Write-Info "  - Docker GPU support not configured"
        Write-Info "  - nvidia-container-toolkit not installed"
    }
}

function Invoke-SetupDbCommand {
    Write-Info "Setting up database migration..."
    if (Test-Path ".\setup-db.ps1") {
        & ".\setup-db.ps1" @RemainingArgs
    } else {
        Write-Error "setup-db.ps1 not found"
    }
}

function Invoke-ComposeCommand {
    Write-Info "Passing command to docker-compose..."
    docker-compose -f $ComposeFile @RemainingArgs
}

# Main execution
function Main {
    if ($Help) {
        Show-Help
        exit 0
    }
    
    Write-Info "=== Whisper Server Docker Runner ==="
    Write-Info "Command: $Command"
    
    switch ($Command) {
        "start" { 
            Invoke-StartCommand 
        }
        "stop" { 
            Invoke-StopCommand 
        }
        "restart" { 
            Invoke-RestartCommand 
        }
        "logs" { 
            Invoke-LogsCommand 
        }
        "status" { 
            Invoke-StatusCommand 
        }
        "shell" { 
            Invoke-ShellCommand 
        }
        "clean" { 
            Invoke-CleanCommand 
        }
        "build" { 
            Invoke-BuildCommand 
        }
        "models" { 
            Invoke-ModelsCommand 
        }
        "gpu-test" { 
            Invoke-GpuTestCommand 
        }
        "setup-db" { 
            Invoke-SetupDbCommand 
        }
        "compose" { 
            Invoke-ComposeCommand 
        }
        "help" { 
            Show-Help 
        }
        default {
            Write-Warn "Unknown command: $Command"
            Show-Help
            exit 1
        }
    }
}

# Execute main function
Main