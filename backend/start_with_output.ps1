# PowerShell script to start both Whisper server and Python backend with visible output
# This script uses PowerShell's Start-Process to run both servers and show their output

# Set the port for Python backend (default: 5167)
$portPython = 5167
if ($args.Count -gt 0) {
    $portPython = $args[0]
}

# Set the port for Whisper server (default: 8178)
$portWhisper = 8178
if ($args.Count -gt 1) {
    $portWhisper = $args[1]
}

Write-Host "====================================="
Write-Host "Conversationaly Backend Startup"
Write-Host "====================================="
Write-Host "Python Backend Port: $portPython"
Write-Host "Whisper Server Port: $portWhisper"
Write-Host "====================================="
Write-Host ""

# Kill any existing whisper-server.exe processes
$whisperProcesses = Get-Process -Name "whisper-server" -ErrorAction SilentlyContinue
if ($whisperProcesses) {
    Write-Host "Stopping existing Whisper server processes..."
    $whisperProcesses | ForEach-Object { $_.Kill() }
    Start-Sleep -Seconds 1
}

# Kill any existing python.exe processes
$pythonProcesses = Get-Process -Name "python" -ErrorAction SilentlyContinue
if ($pythonProcesses) {
    Write-Host "Stopping existing Python processes..."
    $pythonProcesses | ForEach-Object { $_.Kill() }
    Start-Sleep -Seconds 1
}

# Check if whisper-server-package exists, create if not
if (-not (Test-Path "whisper-server-package")) {
    Write-Host "whisper-server-package directory not found."
    
    # Check if whisper-custom exists and has a public folder to copy
    if (Test-Path "whisper-custom\public") {
        Write-Host "Found whisper-custom\public folder. Creating whisper-server-package and copying public folder..."
        New-Item -ItemType Directory -Path "whisper-server-package" -Force | Out-Null
        
        # Copy public folder from whisper-custom
        Write-Host "Copying public folder from whisper-custom..."
        Copy-Item -Path "whisper-custom\public" -Destination "whisper-server-package\public" -Recurse -Force
        Write-Host "Public folder copied successfully."
    } else {
        Write-Host "Creating whisper-server-package directory..."
        New-Item -ItemType Directory -Path "whisper-server-package" -Force | Out-Null
        
        # Create public folder with basic index.html
        Write-Host "Creating public folder with default index.html..."
        New-Item -ItemType Directory -Path "whisper-server-package\public" -Force | Out-Null
        
        # Create a simple index.html file
        $indexContent = @"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Whisper Server</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            padding: 20px;
            box-sizing: border-box;
        }
        .container {
            text-align: center;
            background: rgba(255, 255, 255, 0.1);
            padding: 40px;
            border-radius: 20px;
            backdrop-filter: blur(10px);
            box-shadow: 0 8px 32px 0 rgba(31, 38, 135, 0.37);
            max-width: 600px;
        }
        h1 {
            font-size: 2.5em;
            margin-bottom: 20px;
            text-shadow: 2px 2px 4px rgba(0,0,0,0.2);
        }
        p {
            font-size: 1.2em;
            line-height: 1.6;
            margin-bottom: 30px;
        }
        .status {
            display: inline-block;
            padding: 10px 20px;
            background: rgba(255, 255, 255, 0.2);
            border-radius: 50px;
            font-weight: bold;
        }
        .status.running {
            background: rgba(72, 187, 120, 0.8);
        }
        .info {
            margin-top: 30px;
            padding: 20px;
            background: rgba(0, 0, 0, 0.2);
            border-radius: 10px;
        }
        .info h2 {
            font-size: 1.3em;
            margin-bottom: 15px;
        }
        .endpoint {
            background: rgba(255, 255, 255, 0.1);
            padding: 8px 15px;
            border-radius: 5px;
            margin: 5px 0;
            font-family: 'Courier New', monospace;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🎙️ Whisper Server</h1>
        <p>Speech-to-Text Service</p>
        <div class="status running">Server Running</div>
        
        <div class="info">
            <h2>API Endpoints</h2>
            <div class="endpoint">POST /inference - Transcribe audio</div>
            <div class="endpoint">GET /load - Load model</div>
            <div class="endpoint">GET /models - List available models</div>
        </div>
        
        <div class="info">
            <h2>Service Information</h2>
            <p style="margin: 10px 0;">This is the Whisper speech recognition server.<br>
            It provides real-time transcription services for audio files.</p>
        </div>
    </div>
</body>
</html>
"@
        Set-Content -Path "whisper-server-package\public\index.html" -Value $indexContent
        Write-Host "Default index.html created successfully."
    }
} else {
    # whisper-server-package exists, but check if it has a public folder
    if (-not (Test-Path "whisper-server-package\public")) {
        # Check if whisper-custom has a public folder to copy
        if (Test-Path "whisper-custom\public") {
            Write-Host "Copying public folder from whisper-custom to existing whisper-server-package..."
            Copy-Item -Path "whisper-custom\public" -Destination "whisper-server-package\public" -Recurse -Force
            Write-Host "Public folder copied successfully."
        } else {
            # Create default public folder
            Write-Host "Creating public folder with default index.html..."
            New-Item -ItemType Directory -Path "whisper-server-package\public" -Force | Out-Null
            
            # Create a simple index.html file
            $indexContent = @"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Whisper Server</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
            padding: 20px;
            box-sizing: border-box;
        }
        .container {
            text-align: center;
            background: rgba(255, 255, 255, 0.1);
            padding: 40px;
            border-radius: 20px;
            backdrop-filter: blur(10px);
            box-shadow: 0 8px 32px 0 rgba(31, 38, 135, 0.37);
            max-width: 600px;
        }
        h1 {
            font-size: 2.5em;
            margin-bottom: 20px;
            text-shadow: 2px 2px 4px rgba(0,0,0,0.2);
        }
        p {
            font-size: 1.2em;
            line-height: 1.6;
            margin-bottom: 30px;
        }
        .status {
            display: inline-block;
            padding: 10px 20px;
            background: rgba(255, 255, 255, 0.2);
            border-radius: 50px;
            font-weight: bold;
        }
        .status.running {
            background: rgba(72, 187, 120, 0.8);
        }
        .info {
            margin-top: 30px;
            padding: 20px;
            background: rgba(0, 0, 0, 0.2);
            border-radius: 10px;
        }
        .info h2 {
            font-size: 1.3em;
            margin-bottom: 15px;
        }
        .endpoint {
            background: rgba(255, 255, 255, 0.1);
            padding: 8px 15px;
            border-radius: 5px;
            margin: 5px 0;
            font-family: 'Courier New', monospace;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🎙️ Whisper Server</h1>
        <p>Speech-to-Text Service</p>
        <div class="status running">Server Running</div>
        
        <div class="info">
            <h2>API Endpoints</h2>
            <div class="endpoint">POST /inference - Transcribe audio</div>
            <div class="endpoint">GET /load - Load model</div>
            <div class="endpoint">GET /models - List available models</div>
        </div>
        
        <div class="info">
            <h2>Service Information</h2>
            <p style="margin: 10px 0;">This is the Whisper speech recognition server.<br>
            It provides real-time transcription services for audio files.</p>
        </div>
    </div>
</body>
</html>
"@
            Set-Content -Path "whisper-server-package\public\index.html" -Value $indexContent
            Write-Host "Default index.html created successfully."
        }
    }
}

# Check if whisper-server.exe exists, download if not
if (-not (Test-Path "whisper-server-package\whisper-server.exe")) {
    Write-Host "whisper-server.exe not found. Fetching latest release..."
    
    try {
        # Fetch the latest release information from GitHub API
        Write-Host "Getting latest release information from GitHub..."
        $headers = @{}
        # Add User-Agent header to avoid API rate limiting
        $headers["User-Agent"] = "PowerShell-Script"
        
        $apiUrl = "https://api.github.com/repos/Zackriya-Solutions/meeting-minutes/releases/latest"
        $releaseInfo = Invoke-RestMethod -Uri $apiUrl -Headers $headers -UseBasicParsing
        
        $tagName = $releaseInfo.tag_name
        Write-Host "Latest release tag: $tagName"
        
        # Construct the download URL with the actual tag
        $downloadUrl = "https://github.com/Zackriya-Solutions/meeting-minutes/releases/download/$tagName/whisper-server.exe"
        $destinationPath = "whisper-server-package\whisper-server.exe"
        
        # Download the file
        Write-Host "Downloading whisper-server.exe from release $tagName..."
        Invoke-WebRequest -Uri $downloadUrl -OutFile $destinationPath -UseBasicParsing
        
        # Unblock the downloaded file (Windows security feature)
        Write-Host "Unblocking downloaded file..."
        Unblock-File -Path $destinationPath
        
        Write-Host "whisper-server.exe downloaded and unblocked successfully from release $tagName."
    } catch {
        Write-Host "Error: Failed to download whisper-server.exe"
        Write-Host "Error details: $_"
        
        # Try alternative method - look for any recent release
        Write-Host "Attempting alternative download method..."
        try {
            $allReleasesUrl = "https://api.github.com/repos/Zackriya-Solutions/meeting-minutes/releases"
            $headers = @{"User-Agent" = "PowerShell-Script"}
            $releases = Invoke-RestMethod -Uri $allReleasesUrl -Headers $headers -UseBasicParsing
            
            if ($releases.Count -gt 0) {
                $latestTag = $releases[0].tag_name
                Write-Host "Found release: $latestTag"
                $altDownloadUrl = "https://github.com/Zackriya-Solutions/meeting-minutes/releases/download/$latestTag/whisper-server.exe"
                
                Write-Host "Downloading from: $altDownloadUrl"
                Invoke-WebRequest -Uri $altDownloadUrl -OutFile "whisper-server-package\whisper-server.exe" -UseBasicParsing
                Unblock-File -Path "whisper-server-package\whisper-server.exe"
                Write-Host "whisper-server.exe downloaded successfully from release $latestTag."
            } else {
                throw "No releases found"
            }
        } catch {
            Write-Host "Alternative method also failed."
            Write-Host "Please download whisper-server.exe manually from:"
            Write-Host "https://github.com/Zackriya-Solutions/meeting-minutes/releases"
            Write-Host "And place it in: whisper-server-package\whisper-server.exe"
            exit 1
        }
    }
}

# Check if models directory exists
if (-not (Test-Path "whisper-server-package\models")) {
    Write-Host "Creating models directory..."
    New-Item -ItemType Directory -Path "whisper-server-package\models" -Force | Out-Null
}

# Define available models
$validModels = @(
    "tiny.en", "tiny", "base.en", "base", "small.en", "small", "medium.en", "medium", 
    "large-v1", "large-v2", "large-v3", "large-v3-turbo", 
    "tiny-q5_1", "tiny.en-q5_1", "tiny-q8_0", 
    "base-q5_1", "base.en-q5_1", "base-q8_0", 
    "small.en-tdrz", "small-q5_1", "small.en-q5_1", "small-q8_0", 
    "medium-q5_0", "medium.en-q5_0", "medium-q8_0", 
    "large-v2-q5_0", "large-v2-q8_0", "large-v3-q5_0", 
    "large-v3-turbo-q5_0", "large-v3-turbo-q8_0"
)

# Define available languages
$validLanguages = @(
    "en", "ar", "bg", "bn", "bs", "ca", "cs", "da", "de", "el", "es", "et", "fa", "fi", "fr", "he", "hi", "hr", "hu", "id", "it", "ja", "ko", "lt", "lv", "mk", "ml", "mr", "ms", "mt", "nl", "no", "pl", "pt", "ro", "ru", "sk", "sl", "so", "sq", "sr", "sv", "ta", "te", "th", "tr", "uk", "ur", "vi", "zh"
)

# Select language
if ($args.Count -gt 2) {
    $language = $args[2]
    if ($validLanguages -notcontains $language) {
        Write-Host "Invalid language: $language"
        Write-Host "Available languages: $($validLanguages -join ", ")"
        exit 1
    }
}

# Get available models
$availableModels = @()
if (Test-Path "whisper-server-package\models") {
    $modelFiles = Get-ChildItem "whisper-server-package\models" -Filter "ggml-*.bin" | ForEach-Object { $_.Name }
    foreach ($file in $modelFiles) {
        if ($file -match "ggml-(.*?)\.bin") {
            $availableModels += $matches[1]
        }
    }
}

# Display available models
Write-Host "====================================="
Write-Host "Model Selection"
Write-Host "====================================="
if ($availableModels.Count -gt 0) {
    Write-Host "Available models in models directory:"
    for ($i = 0; $i -lt $availableModels.Count; $i++) {
        Write-Host "  $($i+1). $($availableModels[$i])"
    }
} else {
    Write-Host "No models found in models directory."
}

Write-Host ""
Write-Host "Default model: small"
Write-Host "Default language: en"
$modelInput = Read-Host "Select a model (1-$($availableModels.Count)) or type model name or press Enter for default (small)"
$languageInput = Read-Host "Select a language (1-$($validLanguages.Count)) or type language name or press Enter for default (en)"

# Process the model selection
$modelName = "small"  # Default model
if (-not [string]::IsNullOrWhiteSpace($modelInput)) {
    if ([int]::TryParse($modelInput, [ref]$null)) {
        $index = [int]$modelInput - 1
        if ($index -ge 0 -and $index -lt $availableModels.Count) {
            $modelName = $availableModels[$index]
        } else {
            Write-Host "Invalid selection. Using default model (small)."
        }
    } else {
        # Check if the input is a valid model name
        if ($validModels -contains $modelInput) {
            $modelName = $modelInput
        } else {
            Write-Host "Invalid model name. Using default model (small)."
        }
    }
}

# Process the language selection
$languageName = "en"  # Default language
if (-not [string]::IsNullOrWhiteSpace($languageInput)) {
    if ([int]::TryParse($languageInput, [ref]$null)) {
        $index = [int]$languageInput - 1
        if ($index -ge 0 -and $index -lt $validLanguages.Count) {
            $languageName = $validLanguages[$index]
        } else {
            Write-Host "Invalid selection. Using default language (en)."
        }
    } else {
        # Check if the input is a valid language name
        if ($validLanguages -contains $languageInput) {
            $languageName = $languageInput
        } else {
            Write-Host "Invalid language name. Using default language (en)."
        }
    }
}

Write-Host "Selected language: $languageName"

# Get port number from user
$portInput = Read-Host "Enter Whisper server port number (default: 8178)"
$portWhisper = 8178
if (-not [string]::IsNullOrWhiteSpace($portInput)) {
    if ([int]::TryParse($portInput, [ref]$null)) {
        $portWhisper = [int]$portInput
    } else {
        Write-Host "Invalid port number. Using default port (8178)."
    }
}

Write-Host "Selected port: $portWhisper"

# Check if the model file exists
$modelFile = "whisper-server-package\models\ggml-$modelName.bin"
if (-not (Test-Path $modelFile)) {
    Write-Host "Model file not found: $modelFile"
    Write-Host "Attempting to download model $modelName..."
    
    # Change to backend directory to run download script
    Push-Location $PSScriptRoot
    
    # Download the model using download-ggml-model.cmd
    $process = Start-Process -FilePath "cmd.exe" -ArgumentList "/c download-ggml-model.cmd $modelName" -NoNewWindow -Wait -PassThru
    
    if ($process.ExitCode -eq 0) {
        Write-Host "Model download completed. Checking for downloaded file..."
        
        # Check multiple possible locations for the downloaded model
        $possibleLocations = @(
            "whisper.cpp\models\ggml-$modelName.bin",
            "models\ggml-$modelName.bin",
            "whisper-server-package\models\ggml-$modelName.bin"
        )
        
        $modelFound = $false
        foreach ($location in $possibleLocations) {
            if (Test-Path $location) {
                Write-Host "Found model at: $location"
                
                # Ensure target directory exists
                if (-not (Test-Path "whisper-server-package\models")) {
                    New-Item -ItemType Directory -Path "whisper-server-package\models" -Force | Out-Null
                }
                
                # Copy to target location if not already there
                if ($location -ne "whisper-server-package\models\ggml-$modelName.bin") {
                    Copy-Item $location "whisper-server-package\models\ggml-$modelName.bin" -Force
                    Write-Host "Model copied to whisper-server-package\models directory."
                }
                $modelFound = $true
                break
            }
        }
        
        if (-not $modelFound) {
            Write-Host "Warning: Model download succeeded but file not found in expected locations."
            Write-Host "Falling back to small model..."
            $modelName = "small"
        }
    } else {
        Write-Host "Failed to download model $modelName. Falling back to small model..."
        $modelName = "small"
    }
    
    # If we're falling back to small model, ensure it exists
    if ($modelName -eq "small" -and -not (Test-Path "whisper-server-package\models\ggml-small.bin")) {
        Write-Host "Downloading fallback small model..."
        $smallProcess = Start-Process -FilePath "cmd.exe" -ArgumentList "/c download-ggml-model.cmd small" -NoNewWindow -Wait -PassThru
        
        if ($smallProcess.ExitCode -eq 0) {
            # Check for downloaded small model in possible locations
            $smallLocations = @(
                "whisper.cpp\models\ggml-small.bin",
                "models\ggml-small.bin"
            )
            
            foreach ($location in $smallLocations) {
                if (Test-Path $location) {
                    Copy-Item $location "whisper-server-package\models\ggml-small.bin" -Force
                    Write-Host "Small model downloaded and copied successfully."
                    break
                }
            }
        } else {
            Write-Host "Error: Failed to download fallback small model."
            Write-Host "Please download the model manually and place it in whisper-server-package\models\"
            Pop-Location
            exit 1
        }
    }
    
    Pop-Location
}

Write-Host "====================================="
Write-Host "Starting Conversationaly Backend"
Write-Host "====================================="
Write-Host "Model: $modelName"
Write-Host "Python Backend Port: $portPython"
Write-Host "Whisper Server Port: $portWhisper"
Write-Host "Language: $languageName"
Write-Host "====================================="
Write-Host ""

# Change to script directory to ensure we're in the right location
Push-Location $PSScriptRoot

# Check if virtual environment exists, create if not found
if (-not (Test-Path "venv")) {
    Write-Host "Virtual environment not found in: $PSScriptRoot"
    Write-Host "Creating new virtual environment..."
    
    # Create virtual environment
    $createVenvProcess = Start-Process -FilePath "python" -ArgumentList "-m venv venv" -NoNewWindow -Wait -PassThru
    if ($createVenvProcess.ExitCode -ne 0) {
        Write-Host "Error: Failed to create virtual environment"
        Write-Host "Please ensure Python is installed and accessible from PATH"
        exit 1
    }
    
    Write-Host "Virtual environment created successfully."
    
    # Upgrade pip first
    Write-Host "Upgrading pip..."
    $upgradePipProcess = Start-Process -FilePath "cmd.exe" -ArgumentList "/c venv\Scripts\python.exe -m pip install --upgrade pip" -NoNewWindow -Wait -PassThru
    if ($upgradePipProcess.ExitCode -ne 0) {
        Write-Host "Warning: Failed to upgrade pip, continuing with existing version"
    }
    
    Write-Host "Installing dependencies from requirements.txt..."
    
    # Check if requirements.txt exists
    $requirementsPath = Join-Path $PSScriptRoot "requirements.txt"
    if (Test-Path $requirementsPath) {
        # Install dependencies using the venv's python directly
        Write-Host "Installing packages from: $requirementsPath"
        Write-Host "Installing packages: fastapi, uvicorn, and other dependencies..."
        $installDepsProcess = Start-Process -FilePath "venv\Scripts\python.exe" -ArgumentList "-m pip install -r `"$requirementsPath`"" -NoNewWindow -Wait -PassThru
        if ($installDepsProcess.ExitCode -ne 0) {
            Write-Host "Warning: Failed to install some dependencies from requirements.txt"
            Write-Host "Attempting to install core dependencies individually..."
            
            # Try installing core dependencies one by one
            $coreDeps = @("fastapi", "uvicorn", "python-multipart", "pydantic", "python-dotenv")
            foreach ($dep in $coreDeps) {
                Write-Host "Installing $dep..."
                Start-Process -FilePath "venv\Scripts\python.exe" -ArgumentList "-m pip install $dep" -NoNewWindow -Wait
            }
        } else {
            Write-Host "Dependencies installed successfully."
        }
    } else {
        Write-Host "Warning: requirements.txt not found. Installing core dependencies..."
        # Install minimal required dependencies
        $coreDeps = @("fastapi", "uvicorn[standard]", "python-multipart", "pydantic", "python-dotenv")
        foreach ($dep in $coreDeps) {
            Write-Host "Installing $dep..."
            Start-Process -FilePath "venv\Scripts\python.exe" -ArgumentList "-m pip install $dep" -NoNewWindow -Wait
        }
    }
    
    # Verify FastAPI installation
    Write-Host "Verifying FastAPI installation..."
    $verifyProcess = Start-Process -FilePath "venv\Scripts\python.exe" -ArgumentList "-c `"import fastapi; print('FastAPI installed successfully')`"" -NoNewWindow -Wait -PassThru
    if ($verifyProcess.ExitCode -ne 0) {
        Write-Host "Error: FastAPI installation verification failed"
        Write-Host "Please manually install dependencies using: venv\Scripts\pip.exe install -r requirements.txt"
        exit 1
    }
}

# Check if Python app exists
if (-not (Test-Path "app\main.py")) {
    Write-Host "Error: app\main.py not found"
    Write-Host "Please ensure app\main.py exists in: $PSScriptRoot\app"
    Pop-Location
    exit 1
}

# Restore original directory
Pop-Location

# Start Whisper server in a new window
Write-Host "Starting Whisper server..."
Start-Process -FilePath "cmd.exe" -ArgumentList "/k cd whisper-server-package && whisper-server.exe --model models\ggml-$modelName.bin --host 127.0.0.1 --port $portWhisper --diarize --print-progress --language $languageName" -WindowStyle Normal

# Wait for Whisper server to start
Write-Host "Waiting for Whisper server to start..."
Start-Sleep -Seconds 5

# Check if Whisper server is running
$whisperRunning = $false
try {
    $whisperProcesses = Get-Process -Name "whisper-server" -ErrorAction Stop
    $whisperRunning = $true
    Write-Host "Whisper server started with PID: $($whisperProcesses.Id)"
} catch {
    Write-Host "Error: Whisper server failed to start"
    exit 1
}

# Start Python backend in a new window
Write-Host "Starting Python backend..."
Write-Host "Using virtual environment at: $PSScriptRoot\venv"
Write-Host "Starting with PORT=$portPython"

# Create a batch command that changes to the correct directory first
$pythonCommand = "/k cd /d `"$PSScriptRoot`" && call venv\Scripts\activate.bat && set PORT=$portPython && echo Activated virtual environment && python --version && python app\main.py"
Start-Process -FilePath "cmd.exe" -ArgumentList $pythonCommand -WindowStyle Normal

# Wait for Python backend to start
Write-Host "Waiting for Python backend to start..."
Start-Sleep -Seconds 5

# Check if Python backend is running
$pythonRunning = $false
try {
    $pythonProcesses = Get-Process -Name "python" -ErrorAction Stop
    $pythonRunning = $true
    Write-Host "Python backend started with PID: $($pythonProcesses.Id)"
} catch {
    Write-Host "Error: Python backend failed to start"
    exit 1
}

# Check if services are listening on their ports
Write-Host "Checking if services are listening on their ports..."
$whisperListening = $false
$pythonListening = $false

# Wait a bit longer for services to start listening
Start-Sleep -Seconds 5

# Check Whisper server port
$netstatWhisper = netstat -ano | Select-String -Pattern ":$portWhisper.*LISTENING"
if ($netstatWhisper) {
    $whisperListening = $true
    Write-Host "Whisper server is listening on port $portWhisper"
} else {
    Write-Host "Warning: Whisper server is not listening on port $portWhisper"
}

# Check Python backend port
$netstatPython = netstat -ano | Select-String -Pattern ":$portPython.*LISTENING"
if ($netstatPython) {
    $pythonListening = $true
    Write-Host "Python backend is listening on port $portPython"
} else {
    Write-Host "Warning: Python backend is not listening on port $portPython"
}

# Final status
Write-Host ""
Write-Host "====================================="
Write-Host "Backend Status"
Write-Host "====================================="
Write-Host "Whisper Server: $(if ($whisperRunning) { "RUNNING" } else { "NOT RUNNING" })"
Write-Host "Whisper Server Port: $(if ($whisperListening) { "LISTENING on $portWhisper" } else { "NOT LISTENING on $portWhisper" })"
Write-Host "Python Backend: $(if ($pythonRunning) { "RUNNING" } else { "NOT RUNNING" })"
Write-Host "Python Backend Port: $(if ($pythonListening) { "LISTENING on $portPython" } else { "NOT LISTENING on $portPython" })"
Write-Host ""
Write-Host "The backend services are now running in separate windows."
Write-Host "You can close those windows to stop the services."
Write-Host "====================================="

# Check for frontend installation
Write-Host ""
Write-Host "====================================="
Write-Host "Frontend Application Check"
Write-Host "====================================="

# Check if conversationaly-frontend is installed
$frontendInstalled = $false
$frontendPath = $null

# Check common installation paths for conversationaly-frontend
$possiblePaths = @(
    "$env:LOCALAPPDATA\Programs\conversationaly-frontend\conversationaly-frontend.exe",
    "$env:LOCALAPPDATA\Programs\conversationaly\conversationaly-frontend.exe",
    "$env:ProgramFiles\conversationaly-frontend\conversationaly-frontend.exe",
    "${env:ProgramFiles(x86)}\conversationaly-frontend\conversationaly-frontend.exe",
    "$env:APPDATA\conversationaly-frontend\conversationaly-frontend.exe"
)

foreach ($path in $possiblePaths) {
    if (Test-Path $path) {
        $frontendInstalled = $true
        $frontendPath = $path
        break
    }
}

# Also check if conversationaly is in the registry (properly installed)
try {
    $regPath = Get-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*" -ErrorAction SilentlyContinue | 
               Where-Object { $_.DisplayName -like "*conversationaly*" }
    if (-not $regPath) {
        $regPath = Get-ItemProperty -Path "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*" -ErrorAction SilentlyContinue | 
                   Where-Object { $_.DisplayName -like "*conversationaly*" }
    }
    if ($regPath) {
        $frontendInstalled = $true
        if (-not $frontendPath -and $regPath.InstallLocation) {
            # Clean up the install location path (remove quotes if present)
            $installLocation = $regPath.InstallLocation -replace '^"(.+)"$', '$1'
            
            # Try to find the executable in the install location
            $possibleExeNames = @("conversationaly-frontend.exe", "conversationaly.exe")
            foreach ($exeName in $possibleExeNames) {
                $testPath = Join-Path $installLocation $exeName
                if (Test-Path $testPath) {
                    $frontendPath = $testPath
                    break
                }
            }
        }
    }
} catch {
    # Registry check failed, continue with file system check
}

if ($frontendInstalled) {
    Write-Host "Conversationaly frontend application is installed."
    if ($frontendPath) {
        Write-Host "Location: $frontendPath"
        
        # Ask if user wants to launch the frontend
        $launchFrontend = Read-Host "Do you want to launch the Conversationaly frontend application? (Y/N)"
        if ($launchFrontend -eq 'Y' -or $launchFrontend -eq 'y') {
            Write-Host "Launching Conversationaly frontend..."
            Start-Process -FilePath $frontendPath
            Write-Host "Conversationaly frontend launched successfully."
        }
    }
} else {
    Write-Host "Conversationaly frontend application is not installed."
    Write-Host ""
    $installFrontend = Read-Host "Would you like to download and install the Conversationaly frontend application? (Y/N)"
    
    if ($installFrontend -eq 'Y' -or $installFrontend -eq 'y') {
        Write-Host "Fetching latest release information..."
        
        try {
            # Fetch the latest release information
            $headers = @{"User-Agent" = "PowerShell-Script"}
            $apiUrl = "https://api.github.com/repos/Zackriya-Solutions/meeting-minutes/releases/latest"
            $releaseInfo = Invoke-RestMethod -Uri $apiUrl -Headers $headers -UseBasicParsing
            
            # Find the setup.exe asset - looking for files ending with _x64-setup.exe or similar
            $setupAsset = $releaseInfo.assets | Where-Object { 
                $_.name -like "*setup.exe" -or 
                $_.name -like "*Setup.exe" -or 
                $_.name -like "*_x64-setup.exe" -or
                $_.name -like "*_x64_en-US.msi"
            }
            
            if ($setupAsset) {
                $downloadUrl = $setupAsset.browser_download_url
                $setupFileName = $setupAsset.name
                $tempPath = Join-Path $env:TEMP $setupFileName
                
                Write-Host "Found frontend installer: $setupFileName"
                Write-Host "Downloading from: $downloadUrl"
                Write-Host "This may take a few minutes..."
                
                # Download the installer with progress
                $ProgressPreference = 'SilentlyContinue'
                Invoke-WebRequest -Uri $downloadUrl -OutFile $tempPath -UseBasicParsing
                $ProgressPreference = 'Continue'
                
                # Unblock the downloaded file
                Unblock-File -Path $tempPath
                
                Write-Host "Download completed. Starting installation..."
                Write-Host ""
                Write-Host "IMPORTANT: The installer may require administrator privileges."
                Write-Host "Please follow the installation prompts in the installer window."
                Write-Host ""
                
                # Start the installer
                # The installer will handle UAC elevation if needed
                if ($setupFileName -like "*.msi") {
                    # For MSI files, use msiexec
                    $installerProcess = Start-Process -FilePath "msiexec.exe" -ArgumentList "/i `"$tempPath`"" -PassThru -Wait
                } else {
                    # For EXE files
                    $installerProcess = Start-Process -FilePath $tempPath -PassThru -Wait
                }
                
                if ($installerProcess.ExitCode -eq 0) {
                    Write-Host "Installation completed successfully!"
                    
                    # Check if conversationaly is now installed and launch it
                    Start-Sleep -Seconds 2  # Give the system a moment to register the installation
                    foreach ($path in $possiblePaths) {
                        if (Test-Path $path) {
                            Write-Host "Launching Conversationaly frontend..."
                            Start-Process -FilePath $path
                            break
                        }
                    }
                } elseif ($installerProcess.ExitCode -eq 1602) {
                    Write-Host "Installation was cancelled by the user."
                } else {
                    Write-Host "Installation completed with exit code: $($installerProcess.ExitCode)"
                }
                
                # Clean up temp file
                if (Test-Path $tempPath) {
                    Remove-Item $tempPath -Force -ErrorAction SilentlyContinue
                }
                
            } else {
                Write-Host "Could not find frontend installer in the latest release."
                Write-Host "Available assets in the release:"
                foreach ($asset in $releaseInfo.assets) {
                    Write-Host "  - $($asset.name)"
                }
                Write-Host ""
                Write-Host "Please download the installer manually from:"
                Write-Host "https://github.com/Zackriya-Solutions/meeting-minutes/releases"
            }
            
        } catch {
            Write-Host "Error downloading or installing frontend: $_"
            
            # Try alternative method - look for any recent release
            try {
                Write-Host "Attempting alternative download method..."
                $allReleasesUrl = "https://api.github.com/repos/Zackriya-Solutions/meeting-minutes/releases"
                $releases = Invoke-RestMethod -Uri $allReleasesUrl -Headers @{"User-Agent" = "PowerShell-Script"} -UseBasicParsing
                
                if ($releases.Count -gt 0) {
                    foreach ($release in $releases) {
                        $setupAsset = $release.assets | Where-Object { 
                            $_.name -like "*setup.exe" -or 
                            $_.name -like "*Setup.exe" -or 
                            $_.name -like "*_x64-setup.exe" -or
                            $_.name -like "*_x64_en-US.msi"
                        }
                        if ($setupAsset) {
                            $downloadUrl = $setupAsset.browser_download_url
                            $setupFileName = $setupAsset.name
                            $tempPath = Join-Path $env:TEMP $setupFileName
                            
                            Write-Host "Found frontend installer in release $($release.tag_name): $setupFileName"
                            Write-Host "Downloading..."
                            
                            $ProgressPreference = 'SilentlyContinue'
                            Invoke-WebRequest -Uri $downloadUrl -OutFile $tempPath -UseBasicParsing
                            $ProgressPreference = 'Continue'
                            Unblock-File -Path $tempPath
                            
                            Write-Host "Starting installation..."
                            if ($setupFileName -like "*.msi") {
                                $installerProcess = Start-Process -FilePath "msiexec.exe" -ArgumentList "/i `"$tempPath`"" -PassThru -Wait
                            } else {
                                $installerProcess = Start-Process -FilePath $tempPath -PassThru -Wait
                            }
                            
                            if ($installerProcess.ExitCode -eq 0) {
                                Write-Host "Installation completed successfully!"
                            }
                            
                            # Clean up
                            if (Test-Path $tempPath) {
                                Remove-Item $tempPath -Force -ErrorAction SilentlyContinue
                            }
                            break
                        }
                    }
                    
                    if (-not $setupAsset) {
                        Write-Host "No installer found in any recent releases."
                        Write-Host "Please download the frontend installer manually from:"
                        Write-Host "https://github.com/Zackriya-Solutions/meeting-minutes/releases"
                    }
                }
            } catch {
                Write-Host "Alternative method also failed."
                Write-Host "Please download the frontend installer manually from:"
                Write-Host "https://github.com/Zackriya-Solutions/meeting-minutes/releases"
            }
        }
    }
}

Write-Host ""
Write-Host "====================================="
Write-Host "Setup Complete"
Write-Host "====================================="
