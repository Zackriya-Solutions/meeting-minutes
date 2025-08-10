# Database Setup Script for Meeting App
# Handles existing database discovery and migration

param(
    [string]$DbPath,
    [switch]$Fresh,
    [switch]$Auto,
    [Alias("h")]
    [switch]$Help
)

# Set error action preference
$ErrorActionPreference = "Stop"

# Configuration
$ScriptDir = $PSScriptRoot
$DefaultDbPath = "./meeting_minutes.db"
$DockerDbDir = Join-Path $ScriptDir "data"
$DockerDbPath = Join-Path $DockerDbDir "meeting_minutes.db"

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
Meeting App Database Setup Script

This script helps you set up the database for the Meeting App by:
1. Checking for existing database from previous installations
2. Copying/migrating existing database if found
3. Setting up fresh database for first-time installations

Usage: setup-db.ps1 [OPTIONS]

OPTIONS:
  -DbPath PATH       Specify custom database path to migrate from
  -Fresh             Skip existing database search, create fresh database
  -Auto              Auto-detect and migrate without prompts (if found)
  -Help, -h          Show this help

Examples:
  # Interactive setup (recommended)
  .\setup-db.ps1
  
  # Migrate from custom path
  .\setup-db.ps1 -DbPath "C:\path\to\meeting_minutes.db"
  
  # Fresh installation
  .\setup-db.ps1 -Fresh
  
  # Auto-detect and migrate
  .\setup-db.ps1 -Auto
"@
}

# Function to check if database exists and is valid
function Test-Database {
    param([string]$DbPath)
    
    if (-not (Test-Path $DbPath -PathType Leaf)) {
        return $false
    }
    
    # Simple file validation - just check if it's a .db file and not empty
    $fileInfo = Get-Item $DbPath
    return ($fileInfo.Extension -eq '.db' -and $fileInfo.Length -gt 0)
}

# Function to get database info
function Get-DatabaseInfo {
    param([string]$DbPath)
    
    Write-Info "Database Information:"
    $fileInfo = Get-Item $DbPath
    $sizeKB = [math]::Round($fileInfo.Length / 1KB, 1)
    $sizeMB = [math]::Round($fileInfo.Length / 1MB, 1)
    $sizeDisplay = if ($sizeMB -gt 1) { "${sizeMB} MB" } else { "${sizeKB} KB" }
    
    Write-Host "  Path: $DbPath"
    Write-Host "  Size: $sizeDisplay"
    Write-Host "  Modified: $($fileInfo.LastWriteTime)"
    Write-Host "  Type: SQLite Database (.db file)"
}

# Function to copy database
function Copy-Database {
    param(
        [string]$SourcePath,
        [string]$DestPath
    )
    
    Write-Info "Copying database from $SourcePath to $DestPath"
    
    # Create destination directory if it doesn't exist
    $destDir = Split-Path $DestPath -Parent
    if (-not (Test-Path $destDir -PathType Container)) {
        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
    }
    
    # Copy the database file
    Copy-Item -Path $SourcePath -Destination $DestPath -Force
    
    Write-Info " Database copied successfully"
}

# Function to find existing databases
function Find-ExistingDatabases {
    $foundDbs = @()
    
    # Check default location
    if (Test-Database $DefaultDbPath) {
        $foundDbs += $DefaultDbPath
    }
    
    # Check other common locations (Windows/cross-platform paths)
    $commonPaths = @(
        "$env:USERPROFILE\.meetily\meeting_minutes.db",
        "$env:USERPROFILE\Documents\meetily\meeting_minutes.db",
        "$env:USERPROFILE\Desktop\meeting_minutes.db",
        ".\meeting_minutes.db"
    )
    
    # Add potential HomeBrew paths if on macOS/Linux
    if ($env:HOMEBREW_PREFIX) {
        $commonPaths += "$env:HOMEBREW_PREFIX/Cellar/meetily-backend/*/backend/meeting_minutes.db"
    }
    
    foreach ($pattern in $commonPaths) {
        if ($pattern -contains "*") {
            # Handle wildcard patterns
            try {
                $matches = Get-ChildItem -Path $pattern -ErrorAction SilentlyContinue
                foreach ($match in $matches) {
                    if ($match.FullName -ne $DefaultDbPath -and (Test-Database $match.FullName)) {
                        $foundDbs += $match.FullName
                    }
                }
            } catch {
                # Ignore errors for wildcard patterns
            }
        } else {
            if ($pattern -ne $DefaultDbPath -and (Test-Database $pattern)) {
                $foundDbs += $pattern
            }
        }
    }
    
    return $foundDbs | Select-Object -Unique
}

# Interactive database selection
function Start-InteractiveSetup {
    Write-Host ""
    Write-Info "=== Meeting App Database Setup ==="
    Write-Host ""
    
    Write-Info "Searching for existing databases..."
    $foundDbs = Find-ExistingDatabases
    
    if ($foundDbs.Count -eq 0) {
        Write-Info "No existing databases found."
        Write-Host ""
        Write-Host "Options:"
        Write-Host "1 First-time installation - create fresh database"
        Write-Host "2 I have an existing database at a custom location"
        Write-Host "3 Exit"
        Write-Host ""
        $choice = Read-Host "Please choose an option (1-3)"
        
        switch ($choice) {
            "1" {
                Write-Info "Setting up fresh database for first-time installation"
                New-FreshDatabase
            }
            "2" {
                $customPath = Read-Host "Enter the full path to your existing database"
                if (Test-Database $customPath) {
                    Get-DatabaseInfo $customPath
                    Write-Host ""
                    $confirm = Read-Host "Use this database? (y/N)"
                    if ($confirm -match "^[Yy]$") {
                        Copy-Database $customPath $DockerDbPath
                    } else {
                        Write-Info "Database setup cancelled"
                        exit 0
                    }
                } else {
                    Write-Error "Invalid database file: $customPath"
                    exit 1
                }
            }
            "3" {
                Write-Info "Setup cancelled"
                exit 0
            }
            default {
                Write-Error "Invalid choice"
                exit 1
            }
        }
    } else {
        Write-Info "Found $($foundDbs.Count) existing database(s):"
        Write-Host ""
        
        for ($i = 0; $i -lt $foundDbs.Count; $i++) {
            Write-Host "$($i+1)) $($foundDbs[$i])"
        }
        Write-Host "$($foundDbs.Count+1)) Use custom path"
        Write-Host "$($foundDbs.Count+2)) Fresh installation"
        Write-Host "$($foundDbs.Count+3)) Exit"
        Write-Host ""
        
        $choice = Read-Host "Please choose an option"
        $choiceNum = [int]$choice
        
        if ($choiceNum -ge 1 -and $choiceNum -le $foundDbs.Count) {
            $selectedDb = $foundDbs[$choiceNum-1]
            Write-Host ""
            Get-DatabaseInfo $selectedDb
            Write-Host ""
            $confirm = Read-Host "Use this database? (Y/n)"
            if ($confirm -notmatch "^[Nn]$") {
                Copy-Database $selectedDb $DockerDbPath
            } else {
                Write-Info "Database setup cancelled"
                exit 0
            }
        } elseif ($choiceNum -eq ($foundDbs.Count+1)) {
            $customPath = Read-Host "Enter the full path to your existing database"
            if (Test-Database $customPath) {
                Get-DatabaseInfo $customPath
                Write-Host ""
                $confirm = Read-Host "Use this database? (y/N)"
                if ($confirm -match "^[Yy]$") {
                    Copy-Database $customPath $DockerDbPath
                } else {
                    Write-Info "Database setup cancelled"
                    exit 0
                }
            } else {
                Write-Error "Invalid database file: $customPath"
                exit 1
            }
        } elseif ($choiceNum -eq ($foundDbs.Count+2)) {
            Write-Info "Setting up fresh database for first-time installation"
            New-FreshDatabase
        } elseif ($choiceNum -eq ($foundDbs.Count+3)) {
            Write-Info "Setup cancelled"
            exit 0
        } else {
            Write-Error "Invalid choice"
            exit 1
        }
    }
}

# Function to setup fresh database
function New-FreshDatabase {
    # Create data directory
    if (-not (Test-Path $DockerDbDir -PathType Container)) {
        New-Item -ItemType Directory -Path $DockerDbDir -Force | Out-Null
    }
    
    # Remove existing database if any
    if (Test-Path $DockerDbPath) {
        Remove-Item $DockerDbPath -Force
    }
    
    Write-Info "Fresh database setup complete"
    Write-Info "The application will create a new database on first run"
}

# Auto setup function
function Start-AutoSetup {
    Write-Info "Auto-detecting existing databases..."
    
    if (Test-Database $DefaultDbPath) {
        Write-Info "Found database at default location: $DefaultDbPath"
        Get-DatabaseInfo $DefaultDbPath
        Copy-Database $DefaultDbPath $DockerDbPath
    } else {
        $foundDbs = Find-ExistingDatabases
        if ($foundDbs.Count -gt 0) {
            Write-Info "Found database: $($foundDbs[0])"
            Get-DatabaseInfo $foundDbs[0]
            Copy-Database $foundDbs[0] $DockerDbPath
        } else {
            Write-Info "No existing databases found, setting up fresh installation"
            New-FreshDatabase
        }
    }
}

# Main function
function Main {
    if ($Help) {
        Show-Help
        exit 0
    }
    
    # No sqlite3 required - using simple file-based validation
    
    if ($Fresh) {
        New-FreshDatabase
    } elseif ($DbPath) {
        if (Test-Database $DbPath) {
            Get-DatabaseInfo $DbPath
            Copy-Database $DbPath $DockerDbPath
        } else {
            Write-Error "Invalid database file: $DbPath"
            exit 1
        }
    } elseif ($Auto) {
        Start-AutoSetup
    } else {
        Start-InteractiveSetup
    }
    
    Write-Info '=== Database Setup Complete ==='
    Write-Host "Database location: $DockerDbPath"
    Write-Info 'You can now start the services with: .\run-docker.ps1 compose up -d'
}

# Execute main function
Main