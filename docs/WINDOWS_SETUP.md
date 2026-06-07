# Windows Setup Guide for Meetily

This guide provides detailed instructions for setting up and running Meetily on Windows systems, including how to build the whisper-server binary from source.

## Table of Contents

- [Quick Start (Using Pre-built Binaries)](#quick-start-using-pre-built-binaries)
- [Building from Source](#building-from-source)
- [Troubleshooting](#troubleshooting)

## Quick Start (Using Pre-built Binaries)

### Prerequisites

- Windows 10 or Windows 11 (64-bit)
- At least 4GB of RAM (8GB recommended)
- 2GB of free disk space

### Installation Steps

1. **Download the latest release**
   
   Visit the [Releases page](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest) and download:
   - `Meetily_x64-setup.exe` - Main application installer
   - `whisper-server-windows-x64.zip` - Windows-compatible whisper-server binary (if available)

2. **Install the main application**
   
   - Right-click the downloaded `Meetily_x64-setup.exe` file
   - Select **Properties** → Check **Unblock** → Click **OK**
   - Run the installer
   - If Windows shows a security warning: Click **More info** → **Run anyway**

3. **Set up whisper-server (if pre-built binary is available)**
   
   - Extract `whisper-server-windows-x64.zip` to a location of your choice
   - The package includes:
     - `whisper-server.exe` - The transcription server
     - `models/` - Directory containing the Whisper model
     - `run-server.cmd` - Convenience script to start the server
   
   - To start the server, run `run-server.cmd` or use the command:
     ```cmd
     whisper-server.exe --model models\ggml-small.bin --host 127.0.0.1 --port 8178 --diarize --language en --print-progress
     ```

## Building from Source

If pre-built binaries are not available for your Windows version, or if you want to customize the build, follow these instructions.

### Prerequisites for Building

1. **Visual Studio Build Tools**
   - Download and install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
   - During installation, select "Desktop development with C++"
   - This includes MSVC compiler and Windows SDK

2. **CMake**
   - Download and install from [cmake.org](https://cmake.org/download/)
   - Version 3.5 or higher required
   - During installation, select "Add CMake to system PATH"

3. **Git**
   - Download and install from [git-scm.com](https://git-scm.com/download/win)
   - Required for cloning the repository and submodules

4. **Python 3.8+**
   - Download and install from [python.org](https://www.python.org/downloads/)
   - Make sure to check "Add Python to PATH" during installation

5. **Node.js and pnpm**
   - Download and install Node.js from [nodejs.org](https://nodejs.org/)
   - Install pnpm: `npm install -g pnpm`

6. **Rust**
   - Download and install from [rust-lang.org](https://www.rust-lang.org/tools/install)
   - This is required for building the Tauri application

### Building whisper-server on Windows

1. **Clone the repository**
   ```cmd
   git clone https://github.com/Zackriya-Solutions/meeting-minutes.git
   cd meeting-minutes
   git submodule update --init --recursive
   ```

2. **Navigate to the backend directory**
   ```cmd
   cd backend
   ```

3. **Run the build script**
   ```cmd
   build_whisper.cmd small
   ```
   
   Replace `small` with your preferred model size:
   - `tiny` - Fastest, least accurate (~75MB)
   - `base` - Fast, good accuracy (~142MB)
   - `small` - Balanced (default, ~466MB)
   - `medium` - Slower, better accuracy (~1.5GB)
   - `large-v3` - Best accuracy, slowest (~3GB)

4. **The build process will:**
   - Update git submodules
   - Copy custom server files
   - Build whisper-server.exe using CMake
   - Download the specified Whisper model
   - Create a `whisper-server-package` directory with all necessary files
   - Set up Python virtual environment and install dependencies

5. **After successful build, you'll find:**
   ```
   backend/whisper-server-package/
   ├── whisper-server.exe
   ├── models/
   │   └── ggml-small.bin
   ├── public/
   └── run-server.cmd
   ```

### Building the Full Application

After building whisper-server, you can build the complete Meetily application:

1. **Navigate to the frontend directory**
   ```cmd
   cd ..\frontend
   ```

2. **Install dependencies**
   ```cmd
   pnpm install
   ```

3. **Build the application**
   ```cmd
   pnpm run tauri:build
   ```

4. **The installer will be created at:**
   ```
   frontend/src-tauri/target/release/bundle/nsis/Meetily_<version>_x64-setup.exe
   ```

## Running the Backend Services

### Option 1: Using PowerShell Script (Recommended)

```powershell
cd backend
.\start_with_output.ps1
```

This script will:
- Start the whisper-server
- Start the FastAPI backend
- Display output from both services

### Option 2: Manual Start

**Terminal 1 - Start whisper-server:**
```cmd
cd backend\whisper-server-package
run-server.cmd
```

**Terminal 2 - Start FastAPI backend:**
```cmd
cd backend
python -m venv venv
venv\Scripts\activate
pip install -r requirements.txt
python -m uvicorn app.main:app --host 127.0.0.1 --port 5167
```

### Verifying the Setup

1. **Check whisper-server:**
   - Open browser to `http://localhost:8178`
   - You should see the whisper-server web interface

2. **Check FastAPI backend:**
   - Open browser to `http://localhost:5167/docs`
   - You should see the API documentation

3. **Run the Meetily application:**
   - Launch Meetily from the Start Menu or desktop shortcut
   - The application should connect to both services automatically

## Troubleshooting

### "whisper-server.exe is not recognized"

**Cause:** The binary is not in your PATH or doesn't exist.

**Solution:**
- Make sure you've built whisper-server using `build_whisper.cmd`
- Navigate to the `backend/whisper-server-package` directory before running
- Or use the full path: `C:\path\to\backend\whisper-server-package\whisper-server.exe`

### "MSVCP140.dll is missing"

**Cause:** Visual C++ Redistributable is not installed.

**Solution:**
- Download and install [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)
- Restart your computer after installation

### "CMake configuration failed"

**Cause:** CMake or Visual Studio Build Tools not properly installed.

**Solution:**
- Verify CMake is in PATH: `cmake --version`
- Verify MSVC is installed: Check "Visual Studio Installer" → "Modify" → "Desktop development with C++"
- Restart your terminal after installing

### "Failed to download model"

**Cause:** Network issues or invalid model name.

**Solution:**
- Check your internet connection
- Verify the model name is correct (see list in build script)
- Try downloading manually from [Hugging Face](https://huggingface.co/ggerganov/whisper.cpp)
- Place the model in `backend/whisper.cpp/models/`

### "Port 8178 or 5167 already in use"

**Cause:** Another application is using these ports.

**Solution:**
- Check what's using the port: `netstat -ano | findstr :8178`
- Kill the process or change the port in the configuration
- For whisper-server: `run-server.cmd --port 8179`
- For FastAPI: Edit `backend/temp.env` and change `WHISPER_SERVER_PORT`

### Build succeeds but application crashes

**Cause:** Missing dependencies or incompatible binary.

**Solution:**
- Make sure all Visual C++ Redistributables are installed
- Try building with a smaller model first (`tiny` or `base`)
- Check Windows Event Viewer for detailed error messages
- Ensure you're running 64-bit Windows

### "Python not found" during build

**Cause:** Python is not installed or not in PATH.

**Solution:**
- Install Python from [python.org](https://www.python.org/downloads/)
- During installation, check "Add Python to PATH"
- Restart your terminal after installation
- Verify: `python --version`

## GPU Acceleration on Windows

By default, the Windows build uses CPU-only processing. For GPU acceleration:

### NVIDIA GPUs (CUDA)

1. Install [CUDA Toolkit](https://developer.nvidia.com/cuda-downloads)
2. Build with CUDA support:
   ```cmd
   cd backend\whisper.cpp\build
   cmake .. -DWHISPER_CUDA=ON
   cmake --build . --config Release
   ```

### AMD/Intel GPUs (Vulkan)

1. Install [Vulkan SDK](https://vulkan.lunarg.com/sdk/home)
2. Build with Vulkan support:
   ```cmd
   cd backend\whisper.cpp\build
   cmake .. -DWHISPER_VULKAN=ON
   cmake --build . --config Release
   ```

For more details on GPU acceleration, see the [GPU Acceleration Guide](GPU_ACCELERATION.md).

## Additional Resources

- [Main README](../README.md) - Project overview
- [Building Guide](BUILDING.md) - General build instructions
- [Architecture Documentation](architecture.md) - System architecture
- [API Documentation](../backend/API_DOCUMENTATION.md) - Backend API reference
- [Contributing Guide](../CONTRIBUTING.md) - How to contribute

## Getting Help

If you encounter issues not covered in this guide:

1. Check the [GitHub Issues](https://github.com/Zackriya-Solutions/meeting-minutes/issues) for similar problems
2. Join the [Meetily Discord](https://discord.gg/crRymMQBFH) for community support
3. Open a new issue with:
   - Your Windows version
   - Error messages (full output)
   - Steps to reproduce
   - What you've already tried

## Notes for Developers

### Cross-Platform Considerations

When developing features that interact with whisper-server:

- Use forward slashes (`/`) in paths when possible, or use `os.path.join()` in Python
- Test on both Windows and Unix-like systems
- Be aware of line ending differences (CRLF vs LF)
- Use platform-agnostic commands in scripts

### Automated Builds

The project now includes GitHub Actions workflows that automatically build whisper-server binaries for Windows, macOS, and Linux. These binaries are attached to releases, making it easier for users to get started without building from source.

To trigger a manual build:
1. Go to the [Actions tab](https://github.com/Zackriya-Solutions/meeting-minutes/actions)
2. Select "Build Whisper Server Binaries"
3. Click "Run workflow"
4. Choose the model size and run

The workflow will create platform-specific packages that can be downloaded as artifacts or attached to releases.
