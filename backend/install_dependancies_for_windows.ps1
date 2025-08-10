Write-Host "Installing dependencies..."

try {

    # Install Chocolatey if not already installed
    if (!(Test-Path "$env:ProgramData\chocolatey\choco.exe")) {
        Write-Host "Installing Chocolatey..."
        Set-ExecutionPolicy Bypass -Scope Process -Force
        [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
        Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://chocolatey.org/install.ps1'))
        refreshenv
    } else {
        Write-Host "Chocolatey is already installed"
    }


    # Check Python installation
    Write-Host "Checking Python installation..."
    
    # List of possible Python installation paths
    $pythonPaths = @(
        "C:\Program Files\Python311\python.exe",
        "C:\Python311\python.exe",
        "C:\Users\$env:USERNAME\AppData\Local\Programs\Python\Python311\python.exe",
        "C:\ProgramData\chocolatey\bin\python.exe"
    )
    
    $pythonExe = $null
    foreach ($path in $pythonPaths) {
        if (Test-Path $path) {
            $pythonExe = $path
            Write-Host "Found Python at: $pythonExe"
            break
        }
    }
    
    if ($pythonExe -eq $null) {
        Write-Host "Python not found. Installing Python 3.11..."
        choco install python311 -y --params "/InstallDir:C:\Program Files\Python311 /InstallAllUsers"
        refreshenv
        Start-Sleep -Seconds 5  # Give time for installation to complete
        
        # Check again after installation
        foreach ($path in $pythonPaths) {
            if (Test-Path $path) {
                $pythonExe = $path
                Write-Host "Found Python at: $pythonExe"
                break
            }
        }
        
        if ($pythonExe -eq $null) {
            Write-Host "Python installation failed. Please install Python 3.11 manually."
            exit 1
        }
    }
    
    # Add Python directories to PATH
    $pythonDir = Split-Path -Parent $pythonExe
    $pythonScriptsDir = Join-Path $pythonDir "Scripts"
    
    $currentPath = [System.Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $currentPath.Contains($pythonDir)) {
        Write-Host "Adding Python to PATH..."
        $newPath = "$pythonDir;$pythonScriptsDir;" + $currentPath
        [System.Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$pythonDir;$pythonScriptsDir;" + $env:Path
    }
    
    # Verify Python is working
    try {
        $pythonVersion = & $pythonExe --version 2>&1
        Write-Host "Python is available: $pythonVersion"
    } catch {
        Write-Host "Error verifying Python installation. Please restart your terminal and try again."
        exit 1
    }
    
    # Check pip installation
    Write-Host "Checking pip installation..."
    $pipPath = Join-Path $pythonScriptsDir "pip.exe"
    if (-not (Test-Path $pipPath)) {
        Write-Host "pip not found. Installing pip..."
        & $pythonExe -m ensurepip --upgrade
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Failed to install pip. Please install pip manually."
            exit 1
        }
        Write-Host "pip installed successfully"
        refreshenv
    } else {
        $pipVersion = & $pipPath --version 2>&1
        Write-Host "pip is available: $pipVersion"
    }
    
    # Install Git if not present
    if (!(Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Host "Installing Git..."
        choco install git -y
        refreshenv
    } else {
        Write-Host "Git is already installed"
    }

    # Install CMake if not present
    if (!(Get-Command cmake -ErrorAction SilentlyContinue)) {
        Write-Host "Installing CMake..."
        choco install cmake --installargs 'ADD_CMAKE_TO_PATH=System' -y
        refreshenv
    } else {
        Write-Host "CMake is already installed"
    }

    # Install Visual Studio Build Tools if not present
    $vswhereExe = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (!(Test-Path $vswhereExe) -or !(& $vswhereExe -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64)) {
        Write-Host "Installing Visual Studio Build Tools..."
        choco install visualstudio2022buildtools -y --package-parameters "--add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.Windows11SDK.22000"
        refreshenv
    } else {
        Write-Host "Visual Studio Build Tools are already installed"
    }

    # Setup Visual Studio environment
    Write-Host "Setting up Visual Studio environment..."
    $vsDevCmd = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
    if (Test-Path $vsDevCmd) {
        & cmd /c "call `"$vsDevCmd`" -arch=x64 && set" | foreach-object {
            if ($_ -match '^([^=]+)=(.*)') {
                [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2])
            }
        }
    } else {
        Write-Host "Warning: Visual Studio environment setup failed. You may need to run from a Developer Command Prompt."
    }

    # Install Visual Studio Redistributables
    Write-Host "Installing Visual Studio Redistributables..."
    Write-Host "The script requires administrative privileges. You will be prompted to allow this action."
    $installScript = @"
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://vcredist.com/install.ps1'))
"@
    Start-Process powershell -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -Command `"$installScript`""

    # Check if bun is installed
    $bunInstalled = $false
    $bunVersion = ""
    try {
        $bunVersion = (bun --version 2>$null) -replace "[^\d\.]", ""
        if ($bunVersion -as [version] -ge [version]"1.1.43") {
            $bunInstalled = $true
        }
    } catch {}

    if ($bunInstalled) {
        Write-Host "Bun is already installed and meets version requirements"
    } else {
        Write-Host "Installing bun..."
        Invoke-Expression (Invoke-RestMethod -Uri "https://bun.sh/install.ps1")
    }

    Write-Host "Installation Complete"
    Write-Host ""
    Write-Host "to get started:"
    Write-Host "1. restart your terminal"
    Write-Host "2. Run the following commands:"
    Write-Host "cd Documents"
    Write-Host "git clone https://github.com/zackriya-solutions/meeting-minutes.git"
    Write-Host "cd meeting-minutes/backend"
    Write-Host "./build_whisper.cmd"
    Write-Host ""
   
    try {
        $postHogData = @{
            api_key    = ""
            event      = "cli_install"
            properties = @{
                distinct_id = $env:COMPUTERNAME
                version     = $latestRelease.tag_name
                os          = "windows"
                arch        = "x86_64"
            }
        } | ConvertTo-Json

        Write-Host "Tracking installation..."
        Write-Host $postHogData
    } catch {
        # Silently continue if tracking fails
    }

} catch {
    Write-Host "Installation failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
