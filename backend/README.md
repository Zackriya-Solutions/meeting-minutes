# Meetily Backend

FastAPI backend for meeting transcription and analysis

## Features
- Audio file upload and storage
- Real-time Whisper-based transcription with streaming support
- Meeting analysis with LLMs (supports Claude, Groq, and Ollama)
- REST API endpoints

## Requirements
- Python 3.9+
- FFmpeg
- C++ compiler (for Whisper.cpp)
- CMake
- Git (for submodules)
- Ollama running
- API Keys (for Claude or Groq) if planning to use APIS
- ChromaDB

## Installation

### Prerequisites Installation

#### For Windows:
1. **Python 3.9+**:
   - Download and install from [Python.org](https://www.python.org/downloads/)
   - Ensure you check "Add Python to PATH" during installation
   - Verify installation: `python --version`

2. **FFmpeg**:
   - Download from [FFmpeg.org](https://ffmpeg.org/download.html) or install via [Chocolatey](https://chocolatey.org/): `choco install ffmpeg`
   - Add FFmpeg to your PATH environment variable
   - Verify installation: `ffmpeg -version`

3. **C++ Compiler**:
   - Install Visual Studio Build Tools from [Microsoft](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   - Select "Desktop development with C++" workload during installation
   - Verify installation: `cl` (should show the compiler version)

4. **CMake**:
   - Download and install from [CMake.org](https://cmake.org/download/)
   - Ensure you select "Add CMake to the system PATH" during installation
   - Verify installation: `cmake --version`

5. **Git**:
   - Download and install from [Git-scm.com](https://git-scm.com/download/win)
   - Verify installation: `git --version`

6. **Ollama**:
   - Download and install from [Ollama.com](https://ollama.com/download)
   - Start Ollama service
   - Pull required models: `ollama pull mistral` (or your preferred model)
   - Verify installation: `ollama list`

#### For macOS:
1. **Python 3.9+**:
   - Install via Homebrew: `brew install python@3.9`
   - Or download from [Python.org](https://www.python.org/downloads/)
   - Verify installation: `python3 --version`

2. **FFmpeg**:
   - Install via Homebrew: `brew install ffmpeg`
   - Verify installation: `ffmpeg -version`

3. **C++ Compiler**:
   - Install Xcode Command Line Tools: `xcode-select --install`
   - Verify installation: `clang --version`

4. **CMake**:
   - Install via Homebrew: `brew install cmake`
   - Verify installation: `cmake --version`

5. **Git**:
   - Install via Homebrew: `brew install git`
   - Or install Xcode Command Line Tools: `xcode-select --install`
   - Verify installation: `git --version`

6. **Ollama**:
   - Install via Homebrew: `brew install ollama`
   - Or download from [Ollama.com](https://ollama.com/download)
   - Start Ollama service: `ollama serve`
   - Pull required models: `ollama pull mistral` (or your preferred model)
   - Verify installation: `ollama list`

#### For Linux:
Distro-specific commands are provided for Debian/Ubuntu, Fedora, and Arch-based systems.

1.  **Python 3.9+ & Pip:**
    * **Debian/Ubuntu**: `sudo apt update && sudo apt install -y python3 python3-pip python3-venv`
    * **Fedora**: `sudo dnf install -y python3 python3-pip`
    * **Arch**: `sudo pacman -S --needed --noconfirm python python-pip`
    * Verify: `python3 --version`

2.  **FFmpeg**:
    * **Debian/Ubuntu**: `sudo apt install -y ffmpeg`
    * **Fedora**: `sudo dnf install -y ffmpeg`
    * **Arch**: `sudo pacman -S --needed --noconfirm ffmpeg`
    * Verify: `ffmpeg -version`

3.  **C++ Compiler & Build Tools**:
    * **Debian/Ubuntu**: `sudo apt install -y build-essential`
    * **Fedora**: `sudo dnf groupinstall -y "Development Tools"`
    * **Arch**: `sudo pacman -S --needed --noconfirm base-devel`
    * Verify: `g++ --version`

4.  **CMake**:
    * **Debian/Ubuntu**: `sudo apt install -y cmake`
    * **Fedora**: `sudo dnf install -y cmake`
    * **Arch**: `sudo pacman -S --needed --noconfirm cmake`
    * Verify: `cmake --version`

5.  **Git**:
    * **Debian/Ubuntu**: `sudo apt install -y git`
    * **Fedora**: `sudo dnf install -y git`
    * **Arch**: `sudo pacman -S --needed --noconfirm git`
    * Verify: `git --version`

6.  **Ollama**:
    * Install via the official script: `curl -fsSL https://ollama.com/install.sh | sh`
    * (Arch users can also use `sudo pacman -S ollama`)
    * Pull a model: `ollama pull mistral`
    * Verify: `ollama list`

### 2. Python Dependencies
Install Python dependencies:

#### For Windows:
```cmd
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

#### For macOS:
```bash
python3 -m pip install --upgrade pip
python3 -m pip install -r requirements.txt
```
#### For Linux:
It is highly recommended to use a Python virtual environment.
```bash
python3 -m venv venv
source venv/bin/activate
pip install --upgrade pip
pip install -r requirements.txt
```
### 3. Build Whisper Server

#### For Windows:
Run the build script which will:
- Initialize and update git submodules
- Build Whisper.cpp with custom server modifications
- Set up the server package with required files
- Download the selected Whisper model

```cmd
./build_whisper.cmd
```

If no model is specified, the script will prompt you to choose one interactively.

#### For macOS:
```bash
./build_whisper.sh
```

If you encounter permission issues, make the script executable:
```bash
chmod +x build_whisper.sh
./build_whisper.sh
```
#### For Linux:
The build script will automatically install `cmake`, `llvm` and `openmp`.
```bash
chmod +x build_whisper.sh
./build_whisper.sh
```
### 4. Running the Server

#### For Windows:
The PowerShell script provides an interactive way to start the backend services:

```cmd
./start_with_output.ps1
```

Or directly with PowerShell:
```powershell
powershell -ExecutionPolicy Bypass -File start_with_output.ps1
```

The script will:
1. Check and clean up any existing processes
2. Display available models and prompt for selection
3. Download the selected model if not present
4. Start the Whisper server in a new window
5. Start the FastAPI backend in a new window

To stop all services, close the command windows or press Ctrl+C in each window.

#### For macOS:
```bash
./clean_start_backend.sh
```

If you encounter permission issues:
```bash
chmod +x clean_start_backend.sh
./clean_start_backend.sh
```

To stop all services on macOS, press Ctrl+C in the terminal or use:
```bash
pkill -f "whisper-server"
pkill -f "uvicorn main:app"
```
#### For Linux:
```bash
chmod +x clean_start_backend.sh
./clean_start_backend.sh
```

## API Documentation
Access Swagger UI at `http://localhost:5167/docs`

## Services
The backend runs two services:
1. Whisper.cpp Server: Handles real-time audio transcription
2. FastAPI Backend: Manages API endpoints, LLM integration, and data storage

## Platform-Specific Information

### Windows
- The Windows scripts create separate command windows for each service, allowing you to see the output in real-time
- You can check the status of services using `check_status.cmd`
- If you prefer to start services individually:
  - `start_whisper_server.cmd [model]` - Starts just the Whisper server
  - `start_python_backend.cmd [port]` - Starts just the Python backend

### macOS
- The macOS scripts run services in the foreground by default
- To run services in the background, you can use:
  ```bash
  nohup ./whisper-server/whisper-server -m ./models/ggml-base.en.bin -p 8178 > whisper.log 2>&1 &
  nohup uvicorn main:app --host 0.0.0.0 --port 5167 > backend.log 2>&1 &
  ```
- To check running services: `ps aux | grep -E "whisper-server|uvicorn"`
- To view logs: `tail -f whisper.log` or `tail -f backend.log`

### Linux
- The Linux scripts run all services in the foreground of a single terminal.
- `clean_start_backend.sh` will handle process cleanup on startup.
- To check running services: `ps aux | grep -E "whisper-server|uvicorn"`
- To stop all services on Linux, press Ctrl+C in the terminal or use:
```
pkill -f "whisper-server"
pkill -f "uvicorn main:app"
```

## Troubleshooting

### Common Issues on Windows
- If you see "whisper-server.exe not found", run `build_whisper.cmd` first
- If a model fails to download, try running `download-ggml-model.cmd [model]` directly
- If services don't start, check if ports 8178 (Whisper) and 5167 (Backend) are available
- Ensure you have administrator privileges when running the scripts
- If PowerShell script execution is blocked, run PowerShell as administrator and use:
  ```powershell
  Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process
  ```
- If you encounter "Access is denied" errors, try running Command Prompt as administrator
- For Visual Studio Build Tools issues, try reinstalling with the correct C++ components
- If CMake can't find the compiler, ensure Visual Studio Build Tools are properly installed and PATH variables are set

### Common Issues on macOS
- If scripts fail with "Permission denied", run `chmod +x script_name.sh` to make them executable
- If you see "command not found: python", use `python3` instead
- If building Whisper fails with compiler errors, ensure Xcode Command Line Tools are installed
- For "Port already in use" errors, find and kill the process using:
  ```bash
  lsof -i :5167  # For backend port
  lsof -i :8178  # For Whisper server port
  kill -9 PID    # Replace PID with the actual process ID
  ```
- If Ollama fails to start, check if the service is running with `ps aux | grep ollama`
- For library loading issues, ensure all dependencies are properly installed
- If you encounter "xcrun: error", reinstall the Xcode Command Line Tools:
  ```bash
  xcode-select --install
  ```
- For M1/M2 Macs, ensure you're using ARM-compatible versions of software

### Common Issues on Linux
- If scripts fail with "Permission denied", run `chmod +x script_name.sh` to make them executable.
- If building Whisper fails with compiler errors, ensure you have installed the correct build tools for your distribution (`build-essential`, `base-devel`, etc.).
- For "Port already in use" errors, find and kill the process using:
  ```bash
  sudo lsof -i :5167  # For backend port
  sudo lsof -i :8178  # For Whisper server port
  sudo kill -9 PID    # Replace PID with the actual process ID

### General Troubleshooting
- If services fail to start, the script will automatically clean up processes
- Check logs for detailed error messages
- Ensure all ports (5167 for backend, 8178 for Whisper) are available
- Verify API keys if using Claude or Groq
- For Ollama, ensure the Ollama service is running and models are pulled
- If build fails:
  - Ensure all dependencies (CMake, C++ compiler) are installed
  - Check if git submodules are properly initialized
  - Verify you have write permissions in the directory
