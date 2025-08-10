# Backend Scripts Documentation

This comprehensive document details all the `.cmd`, `.ps1`, and `.sh` scripts in the backend directory, their purposes, usage patterns, interactions, and available options.

## Overview

The backend contains three categories of deployment approaches:

1. **Native Development Scripts** - Direct execution on the host system
2. **Docker-Based Scripts** - Containerized deployment with cross-platform support
3. **Legacy/Utility Scripts** - Supporting utilities and older approaches

## Quick Start Guide: Building and Running the Backend

### Native Approach 
### (Direct Host Execution recommended for better transcription speed)

#### Windows

**Prerequisites:**
- Python 3.8+ installed and in PATH
- Git with submodules support
- CMake and Visual Studio Build Tools
- PowerShell 5.0+ (for advanced scripts)

**Build Process:**
```cmd
# 1. Navigate to backend directory
cd backend

# 2. Build whisper.cpp and setup environment
build_whisper.cmd small

# 3. Start services (interactive mode)
start_with_output.ps1

# Alternative: Use clean_start_backend.cmd
clean_start_backend.cmd

```

**What happens during build:**
- Git submodules are updated (`whisper.cpp`)
- Custom server files are copied from `whisper-custom/server/`
- whisper.cpp is compiled using CMake and Visual Studio
- Python virtual environment is created in `venv/`
- Dependencies are installed from `requirements.txt`
- Whisper model is downloaded (e.g., `ggml-small.bin` ~244MB)
- `whisper-server-package/` is created with all necessary files

#### macOS

**Prerequisites:**
- Xcode Command Line Tools: `xcode-select --install`
- Homebrew: `/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"`
- Python 3.8+: `brew install python3`
- Dependencies: `brew install cmake llvm libomp`

**Build Process:**
```bash
# 1. Navigate to backend directory
cd backend

# 2. Build whisper.cpp and setup environment
./build_whisper.sh small

# 3. Start services (interactive mode)
./clean_start_backend.sh
```

**macOS-Specific Optimizations:**
- Uses `libomp` for OpenMP acceleration
- LLVM compiler optimizations for Apple Silicon
- Automatic detection of M1/M2 vs Intel architecture
- Optimized thread allocation for Apple Silicon cores

### Docker Approach (Containerized - easy to use)

#### Windows (PowerShell)

**Prerequisites:**
- Docker Desktop for Windows
- PowerShell 5.0+ (Windows 10/11 built-in)
- WSL2 (for optimal performance)

**Quick Start:**
```powershell
# 1. Navigate to backend directory
cd backend

# 2. Build whisper.cpp and setup environment
.\build-docker.ps1 cpu -NoCache

# 3. Interactive setup with all options
.\run-docker.ps1 start -Interactive

# 4. Or use defaults for quick start
.\run-docker.ps1 start -Detach
```

**Interactive Setup Flow:**
1. **Previous Settings**: If found, offers to reuse, customize, or use defaults
2. **Model Selection**: Choose from 20+ models with size/accuracy guidance
3. **Language**: Select from 40+ supported languages
4. **Ports**: Configure Whisper (8178) and App (5167) ports with conflict detection
5. **Database**: Fresh installation or migrate from existing database
6. **GPU**: Auto-detect and configure GPU acceleration
7. **Features**: Enable translation, diarization, progress display

**Advanced Configuration:**
```powershell
# Start with specific model and GPU
.\run-docker.ps1 start -Model large-v3 -Port 8081 -Gpu -Language de -Detach


# Monitor and manage
.\run-docker.ps1 logs -Service whisper -Follow
.\run-docker.ps1 status
.\run-docker.ps1 gpu-test
```

#### macOS (Bash)

**Prerequisites:**
- Docker Desktop for Mac
- Terminal with Bash 4.0+
- Optional: iTerm2 for better terminal experience

**Quick Start:**
```bash
# 1. Navigate to backend directory
cd backend

# 2. Build whisper.cpp and setup environment
./build-docker.sh cpu -no-cache

# 3. Interactive setup
./run-docker.sh start --interactive

# 4. Or quick start with defaults
./run-docker.sh start --detach
```

**macOS-Specific Features:**
- Automatic detection of Apple Silicon vs Intel
- Docker profile selection for optimal performance
- Volume mounting optimized for macOS file system
- Native notification support for service status

**Advanced Usage:**
```bash
# Start with specific configuration
./run-docker.sh start --model large-v3 --gpu --language en --detach

# Database setup with auto-detection
./run-docker.sh setup-db --auto

# Build macOS-optimized images
./run-docker.sh build macos

# System monitoring
./run-docker.sh logs --service whisper --follow
./run-docker.sh status
./run-docker.sh models download base.en
```

### Comparison: Native vs Docker

| Aspect | Native | Docker |
|--------|---------|---------|
| **Performance** | Optimal (direct hardware access) | Not optimal (container overhead ~5-10%) |
| **Setup Time** | Medium (compile time ~5-10 min) | Fast (pre-built images) |
| **Dependencies** | Manual installation required | Isolated, no host pollution |
| **GPU Support** | Full native support | NVIDIA only (Windows/Linux) |
| **Portability** | Platform-specific builds | Universal containers |
| **Development** | Faster iteration cycles | Consistent environments |
| **Troubleshooting** | Direct system access | Container logs and debugging |
| **Resource Usage** | Lower memory footprint | Higher memory usage |
| **Isolation** | Shared host environment | Complete isolation |

### Recommended Approaches

#### For Development
**Windows**: Native approach for fastest iteration
```cmd
build_whisper.cmd small
start_with_output.ps1
```

**macOS**: Docker approach for consistency
```bash
build-docker.sh cpu -no-cache
./run-docker.sh start --interactive
```

#### For Production
**Both Platforms**: Docker with pre-built models
```bash
# Pre-download models
./run-docker.sh models download large-v3

# Start in production mode
./run-docker.sh start --model large-v3 --detach --language auto
```

#### For Distribution
**Both Platforms**: Docker with registry
```bash
./run-docker.sh build both --registry ghcr.io/yourorg --push
```

### Service URLs and Endpoints

After successful startup, services are available at:

- **Whisper Server**: http://localhost:8178
  - Health check: `GET /`
  - Transcription: `POST /inference`
  - WebSocket: `ws://localhost:8178/`

- **Meeting App**: http://localhost:5167
  - API docs: http://localhost:5167/docs
  - Health check: `GET /get-meetings`
  - WebSocket: `ws://localhost:5167/ws`

### Troubleshooting Common Issues

#### Native Build Issues
```bash
# Windows: CMake not found
# Solution: Install Visual Studio Build Tools

# macOS: Compilation errors
brew install cmake llvm libomp
export CC=/opt/homebrew/bin/clang
export CXX=/opt/homebrew/bin/clang++

# Python dependency issues
python -m pip install --upgrade pip
pip install -r requirements.txt --force-reinstall
```

#### Docker Issues
```bash
# Port conflicts
./run-docker.sh stop
# Check with: netstat -an | findstr :8178

# GPU not detected (Windows)
# Enable WSL2 integration in Docker Desktop
# Install nvidia-container-toolkit

# Model download failures
# Check internet connection and disk space
./run-docker.sh models download base.en
```

## Native Development Scripts (.cmd, .sh)

### Core Build Scripts

#### `build_whisper.cmd` / `build_whisper.sh`
**Purpose**: Primary build script that compiles whisper.cpp, sets up Python environment, and creates the whisper-server package.

**Key Features**:
- Updates git submodules for whisper.cpp
- Copies custom server files from `whisper-custom/server/`
- Compiles whisper.cpp with CMake (Windows) or make (Unix)
- Creates whisper-server-package with executable and models
- Sets up Python virtual environment and installs dependencies
- Supports interactive model selection

**Usage**:
```bash
# With specific model
./build_whisper.sh small

# Interactive mode (prompts for model)
./build_whisper.sh
```

**Options**:
- `MODEL_NAME`: First argument specifies whisper model to download (tiny, base, small, medium, large-v1, large-v2, large-v3, etc.)
- Auto-downloads models if not present
- Creates executable run scripts in the package

#### `clean_start_backend.cmd` / `clean_start_backend.sh`
**Purpose**: Complete environment cleanup and service startup script that ensures clean state before launching.

**Key Features**:
- Kills existing whisper-server and Python backend processes
- Validates all required directories and files exist
- Interactive model selection with fallback downloading
- Port configuration and conflict resolution
- Starts both whisper server and Python backend
- Comprehensive error handling and logging

**Usage**:
```bash
# With specific model
./clean_start_backend.sh large-v3

# Interactive mode
./clean_start_backend.sh
```

**Options**:
- `MODEL_NAME`: First argument for model selection
- Interactive prompts for model, language, and port selection
- Automatic port conflict detection and resolution
- Process cleanup with user confirmation

#### `start_python_backend.cmd`
**Purpose**: Standalone Python backend launcher for Windows.

**Features**:
- Activates virtual environment
- Validates FastAPI installation
- Configurable port (default: 5167)
- Error checking for all dependencies

**Usage**:
```cmd
start_python_backend.cmd [PORT]
```

#### `start_whisper_server.cmd`
**Purpose**: Standalone whisper server launcher for Windows.

**Features**:
- Validates whisper-server-package structure
- Model validation and listing
- Configurable model selection
- Host and port configuration

**Usage**:
```cmd
start_whisper_server.cmd [MODEL_NAME]
```

### Model Management Scripts

#### `download-ggml-model.cmd` / `download-ggml-model.sh`
**Purpose**: Downloads pre-converted whisper models from HuggingFace.

**Features**:
- Comprehensive model catalog (39 different models)
- Multiple model sizes: tiny (~39MB) to large-v3-turbo (~1550MB)
- Quantized variants (q5_1, q8_0) for smaller file sizes
- Special tdrz models for speaker diarization
- Automatic source URL switching based on model type
- PowerShell BITS transfer (Windows) or curl/wget (Unix)

**Usage**:
```bash
# Download specific model
./download-ggml-model.sh base.en

# View available models
./download-ggml-model.sh
```

**Available Models**:
- **tiny series**: tiny, tiny.en, tiny-q5_1, tiny.en-q5_1, tiny-q8_0
- **base series**: base, base.en, base-q5_1, base.en-q5_1, base-q8_0
- **small series**: small, small.en, small-q5_1, small.en-q5_1, small-q8_0, small.en-tdrz
- **medium series**: medium, medium.en, medium-q5_0, medium.en-q5_0, medium-q8_0
- **large series**: large-v1, large-v2, large-v3, large-v3-turbo (with quantized variants)

## Docker-Based Scripts (.ps1, .sh)

### Primary Docker Management

#### `run-docker.ps1` / `run-docker.sh`
**Purpose**: Comprehensive Docker deployment manager with advanced user experience features.

**Key Features**:
- Interactive setup with preference persistence
- Automatic GPU detection and mode selection
- Database migration from existing installations
- Multi-service orchestration (whisper + meeting app)
- Advanced logging and monitoring
- Cross-platform compatibility (Windows/macOS/Linux)

**Commands**:
```powershell
# Interactive setup with all options
.\run-docker.ps1 start -Interactive

# Quick start with defaults
.\run-docker.ps1 start

# Start with specific configuration
.\run-docker.ps1 start -Model large-v3 -Port 8081 -Gpu -Language es -Detach

# Database setup
.\run-docker.ps1 setup-db --auto

# View logs with options
.\run-docker.ps1 logs -Service whisper -Follow

# System management
.\run-docker.ps1 status
.\run-docker.ps1 clean -All
.\run-docker.ps1 gpu-test
```

**Advanced Options**:
- **Model Selection**: 20+ models with size/accuracy guidance
- **Port Configuration**: Automatic conflict detection
- **GPU Management**: Auto-detection with fallback
- **Language Support**: 40+ languages with auto-detection
- **Database Options**: Migration from existing installations or fresh setup
- **Preference Persistence**: Saves configuration for future runs
- **Service Management**: Individual service control and monitoring

#### `build-docker.ps1` / `build-docker.sh`
**Purpose**: Multi-platform Docker image builder with intelligent platform detection.

**Key Features**:
- Cross-platform builds (CPU, GPU, macOS-optimized)
- Automatic platform detection and optimization
- Multi-architecture support (AMD64, ARM64)
- Registry management with tagging strategies
- Build validation and verification

**Build Types**:
```powershell
# CPU-only build (universal compatibility)
.\build-docker.ps1 cpu

# GPU-enabled build (CUDA support)
.\build-docker.ps1 gpu

# macOS-optimized build (Apple Silicon)
.\build-docker.ps1 macos

# Build both CPU and GPU versions
.\build-docker.ps1 both
```

**Advanced Options**:
```powershell
# Multi-platform build with registry push
.\build-docker.ps1 gpu -Registry ghcr.io/user -Push -Platforms linux/amd64,linux/arm64

# Custom build with specific CUDA version
.\build-docker.ps1 gpu -BuildArgs "CUDA_VERSION=12.1.1"

# Build with cache optimization
.\build-docker.ps1 cpu -NoCache -Tag custom-build
```

### Database Management

#### `setup-db.ps1` / `setup-db.sh`
**Purpose**: Database setup and migration utility for Docker deployments.

**Features**:
- **Auto-discovery**: Finds existing databases from previous installations
- **Interactive Migration**: Step-by-step database selection and validation
- **Fresh Installation**: Clean database setup for new deployments
- **Validation**: SQLite database integrity checking
- **Cross-platform Paths**: Handles Windows, macOS, and Linux path conventions

**Usage Modes**:
```powershell
# Interactive setup (recommended)
.\setup-db.ps1

# Auto-detect and migrate
.\setup-db.ps1 -Auto

# Fresh installation
.\setup-db.ps1 -Fresh

# Custom database path
.\setup-db.ps1 -DbPath "C:\path\to\database.db"
```

**Search Locations**:
- HomeBrew installations: `/opt/homebrew/Cellar/meetily-backend/*/`
- User directories: `~/.meetily/`, `~/Documents/meetily/`, `~/Desktop/`
- Current directory and data directory
- Custom paths with validation

### PowerShell-Specific Scripts

#### `start_with_output.ps1`
**Purpose**: Advanced service launcher with comprehensive user interface for Windows.

**Features**:
- **Model Management**: Interactive selection from 70+ available models
- **Language Selection**: Support for 40+ languages with user-friendly interface
- **Port Management**: Automatic conflict detection and resolution
- **Process Management**: Intelligent cleanup of existing services
- **Service Validation**: Health checks and connectivity testing
- **User Experience**: Rich console interface with progress indicators

**Interactive Features**:
- Model size guidance (speed vs accuracy trade-offs)
- Automatic model downloading with progress tracking
- Language selection with common languages highlighted
- Port conflict resolution with automatic suggestions
- Service status monitoring with detailed feedback

## Container Support Scripts

### `docker/entrypoint.sh`
**Purpose**: Docker container initialization and runtime management.

**Key Features**:
- **GPU Detection**: Multi-vendor GPU support (NVIDIA, AMD, Intel)
- **Model Management**: Automatic downloading with progress tracking
- **Thread Optimization**: CPU core detection and optimal thread allocation
- **Configuration Validation**: Environment variable processing and validation
- **Fallback Strategies**: Graceful degradation for missing models or hardware

**Environment Variables**:
```bash
WHISPER_MODEL=models/ggml-base.en.bin    # Model path
WHISPER_HOST=0.0.0.0                     # Server host
WHISPER_PORT=8178                        # Server port
WHISPER_THREADS=0                        # Thread count (0=auto)
WHISPER_USE_GPU=true                     # GPU acceleration
WHISPER_LANGUAGE=en                      # Language code
WHISPER_TRANSLATE=false                  # Translation to English
WHISPER_DIARIZE=false                    # Speaker diarization
WHISPER_PRINT_PROGRESS=true              # Progress display
WHISPER_DEBUG=false                      # Debug logging
```

**Container Commands**:
```bash
# Start server (default)
docker run whisper-server

# Run diagnostics
docker run whisper-server gpu-test
docker run whisper-server models
docker run whisper-server test

# Shell access
docker run -it whisper-server bash
```

## Script Interactions and Dependencies

### Build Process Flow

1. **`build_whisper.sh/.cmd`** →
   - Initializes git submodules
   - Copies custom server files
   - Compiles whisper.cpp
   - Calls **`download-ggml-model.sh/.cmd`** for model acquisition
   - Sets up Python virtual environment
   - Creates whisper-server-package

2. **`clean_start_backend.sh/.cmd`** →
   - Validates build output from step 1
   - Manages process cleanup
   - Calls **`download-ggml-model.sh/.cmd`** if models missing
   - Starts both whisper server and Python backend

### Docker Deployment Flow

1. **`build-docker.sh/.ps1`** →
   - Copies custom server files (same as native build)
   - Builds Docker images with embedded dependencies
   - Creates tagged images for different platforms

2. **`setup-db.ps1/.sh`** →
   - Discovers and migrates existing databases
   - Prepares data directory for container mounting

3. **`run-docker.sh/.ps1`** →
   - Calls **`build-docker.sh/.ps1`** if images missing
   - Uses **`setup-db.sh/.ps1`** for database preparation
   - Orchestrates multi-container deployment
   - Monitors service health and readiness

4. **`docker/entrypoint.sh`** (inside container) →
   - Handles runtime model downloading
   - Configures hardware-specific optimizations
   - Starts whisper server with optimal settings

### Preference and State Management

#### `run-docker.ps1` Preference System
- **Storage**: `.docker-preferences` file with JSON-like format
- **Persistence**: Saves model, ports, GPU mode, language, features
- **User Experience**: Offers previous settings, customization, or defaults
- **Migration**: Handles database path preferences

#### State Validation Chain
1. **Environment Check**: Validates required directories and files
2. **Process Check**: Identifies and handles conflicting processes
3. **Model Check**: Ensures models are available or downloadable
4. **Port Check**: Validates port availability and resolves conflicts
5. **Service Check**: Monitors startup and health status

## Platform-Specific Considerations

### Windows (.cmd, .ps1)
- **Process Management**: Uses `tasklist`, `taskkill` for process control
- **Port Detection**: `netstat -ano` for port monitoring
- **PowerShell Features**: Rich UI, BITS transfer, advanced error handling
- **Batch File Limitations**: Simple syntax, limited error handling

### Unix/Linux/macOS (.sh)
- **Process Management**: Uses `ps`, `kill`, `pkill` for process control
- **Port Detection**: `lsof`, `netstat` for port monitoring
- **Signal Handling**: Proper SIGTERM/SIGINT handling for graceful shutdown
- **Permission Management**: Executable permissions, file ownership

### Cross-Platform Docker
- **Platform Detection**: Automatic architecture detection (AMD64/ARM64)
- **GPU Support**: NVIDIA CUDA with graceful CPU fallback
- **Volume Mounting**: Host-specific path handling
- **Network Configuration**: Universal port binding with host compatibility

## Error Handling and Recovery

### Graceful Degradation
1. **Missing Models**: Auto-download → Local copy → Fallback model → Error
2. **GPU Unavailable**: GPU requested → CPU fallback → Warning notification
3. **Port Conflicts**: Kill existing → Alternative port → User prompt → Error
4. **Build Failures**: Detailed diagnostics → Cleanup → Recovery suggestions

### Logging and Diagnostics
- **Structured Logging**: Color-coded output with severity levels
- **Progress Tracking**: Real-time feedback for long operations
- **Health Checks**: Service connectivity and readiness validation
- **Debug Mode**: Verbose logging for troubleshooting

### User Guidance
- **Error Messages**: Specific, actionable error descriptions
- **Recovery Steps**: Clear instructions for problem resolution
- **Alternative Approaches**: Multiple deployment options for different scenarios
- **Documentation**: Inline help and comprehensive documentation

## Usage Recommendations

### For Development
1. Use **native scripts** (`build_whisper.sh`, `clean_start_backend.sh`) for fastest iteration
2. Enable debug mode for troubleshooting
3. Use interactive modes for configuration discovery

### For Production
1. Use **Docker approach** (`run-docker.sh/.ps1`) for consistency and isolation
2. Pre-download models to avoid startup delays
3. Use detached mode with proper logging configuration

### For Distribution
1. Use **Docker builds** with multi-platform support
2. Include model management in deployment process
3. Provide database migration path for existing users

This documentation provides comprehensive coverage of all script functionality, interactions, and usage patterns for the Meeting Minutes backend system.