# Meetily Backend

FastAPI backend for meeting transcription and analysis with **Docker distribution system** for easy deployment.

## 📋 Table of Contents
- [⚠️ Important Notes](#️-important-notes)
- [🚀 Quick Start](#-quick-start)
- [🐳 Docker Deployment (Recommended)](#-docker-deployment-recommended)
- [💻 Native Development](#-native-development)
- [🔧 Manual Installation](#-manual-installation)
- [📚 API Documentation](#-api-documentation)
- [🛠️ Troubleshooting](#️-troubleshooting)
- [📖 Complete Script Reference](#-complete-script-reference)

---

## ⚠️ Important Notes

### Audio Processing Requirements
When running in Docker containers, audio processing can drop chunks due to resource limitations:

**Symptoms:**
- Log messages: "Dropped old audio chunk X due to queue overflow"
- Missing or incomplete transcriptions
- Processing delays

**Prevention:**
- Allocate **8GB+ RAM** to Docker containers
- Ensure adequate CPU allocation
- Use appropriate Whisper model size for your hardware
- Monitor container resource usage

---

## 🚀 Quick Start

Choose your preferred deployment method:

### Option 1: Docker (Recommended - Easiest)
```bash
# Navigate to backend directory
cd backend

# Windows (PowerShell)
.\build-docker.ps1 cpu
.\run-docker.ps1 start -Interactive

# macOS/Linux (Bash)
./build-docker.sh cpu
./run-docker.sh start --interactive
```

### Option 2: Native Development (Fastest Performance)
```bash
# Navigate to backend directory
cd backend

# Windows
build_whisper.cmd small
start_with_output.ps1

# macOS/Linux
./build_whisper.sh small
./clean_start_backend.sh
```

**After startup, access:**
- **Whisper Server**: http://localhost:8178
- **Meeting App**: http://localhost:5167 (with API docs at `/docs`)

---

## 🐳 Docker Deployment (Recommended)

Docker provides the easiest setup with automatic dependency management, GPU detection, and cross-platform compatibility.

### Prerequisites
- Docker Desktop (Windows/Mac) or Docker Engine (Linux)
- 8GB+ RAM allocated to Docker
- For GPU: NVIDIA drivers + nvidia-container-toolkit

### Windows (PowerShell)

#### Basic Setup
```powershell
# Build images
.\build-docker.ps1 cpu

# Interactive setup (recommended for first-time users)
.\run-docker.ps1 start -Interactive

# Quick start with defaults
.\run-docker.ps1 start -Detach
```

#### Advanced Configuration
```powershell
# GPU acceleration
.\build-docker.ps1 gpu
.\run-docker.ps1 start -Model large-v3 -Gpu -Language en -Detach

# Custom ports and features
.\run-docker.ps1 start -Port 8081 -AppPort 5168 -Translate -Diarize

# Monitor services
.\run-docker.ps1 logs -Service whisper -Follow
.\run-docker.ps1 status
```

### macOS/Linux (Bash)

#### Basic Setup
```bash
# Build images
./build-docker.sh cpu

# Interactive setup (recommended)
./run-docker.sh start --interactive

# Quick start with defaults
./run-docker.sh start --detach
```

#### Advanced Configuration
```bash
# With specific model and language
./run-docker.sh start --model base --language es --detach

# View logs and status
./run-docker.sh logs --service whisper --follow
./run-docker.sh status

# Database migration from existing installation
./run-docker.sh setup-db --auto
```

### Interactive Setup Features

The interactive mode guides you through:

1. **Model Selection** - Choose from 20+ models with size/accuracy guidance
2. **Language Settings** - Select from 40+ supported languages  
3. **Port Configuration** - Automatic conflict detection and resolution
4. **Database Setup** - Migrate from existing installations or start fresh
5. **GPU Configuration** - Auto-detection and setup
6. **Advanced Features** - Translation, diarization, progress display
7. **Settings Persistence** - Saves preferences for future runs

### Model Size Guide

| Model | Size | Accuracy | Speed | Best For |
|-------|------|----------|-------|----------|
| tiny | ~39 MB | Basic | Fastest | Testing, low resources |
| base | ~142 MB | Good | Fast | General use (recommended) |
| small | ~244 MB | Better | Medium | Better accuracy needed |
| medium | ~769 MB | High | Slow | High accuracy requirements |
| large-v3 | ~1550 MB | Best | Slowest | Maximum accuracy |

### Docker vs Native Comparison

| Aspect | Docker | Native |
|--------|--------|--------|
| **Setup** | Easy (automated) | Manual (requires dependencies) |
| **Performance** | Good (5-10% overhead) | Optimal (direct hardware) |
| **GPU Support** | NVIDIA only | Full native support |
| **Isolation** | Complete | Shared environment |
| **Portability** | Universal | Platform-specific |
| **Updates** | Container replacement | Manual updates |

---

## 💻 Native Development

Native deployment offers optimal performance by running directly on the host system.

### Prerequisites

#### Windows
- Python 3.8+ (in PATH)
- Visual Studio Build Tools (C++ workload)
- CMake
- Git
- PowerShell 5.0+

#### macOS
- Xcode Command Line Tools: `xcode-select --install`
- Homebrew: `/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"`
- Python 3.8+: `brew install python3`
- Dependencies: `brew install cmake llvm libomp`

### Windows Setup

```cmd
# Navigate to backend directory
cd backend

# Build whisper.cpp with model (e.g., 'small', 'base.en', 'large-v3')
build_whisper.cmd small

# Start services interactively
start_with_output.ps1

# Alternative: Clean start
clean_start_backend.cmd
```

**Build Process:**
1. Updates git submodules (`whisper.cpp`)
2. Copies custom server files from `whisper-custom/server/`
3. Compiles whisper.cpp using CMake + Visual Studio
4. Creates Python virtual environment in `venv/`
5. Installs dependencies from `requirements.txt`
6. Downloads specified Whisper model
7. Creates `whisper-server-package/` with all files

### macOS Setup

```bash
# Navigate to backend directory
cd backend

# Build whisper.cpp with model
./build_whisper.sh small

# Start services
./clean_start_backend.sh
```

**macOS Optimizations:**
- OpenMP acceleration with `libomp`
- LLVM compiler optimizations for Apple Silicon
- Automatic M1/M2 vs Intel detection
- Optimized thread allocation for Apple Silicon cores

### Service URLs
- **Whisper Server**: http://localhost:8178
  - Health: `GET /`
  - Transcription: `POST /inference`
  - WebSocket: `ws://localhost:8178/`
- **Meeting App**: http://localhost:5167
  - API docs: http://localhost:5167/docs
  - Health: `GET /get-meetings`
  - WebSocket: `ws://localhost:5167/ws`

---

## 🔧 Manual Installation

If you prefer complete manual control over the installation process.

### System Requirements
- Python 3.9+
- FFmpeg
- C++ compiler (Visual Studio Build Tools/Xcode)
- CMake
- Git (with submodules support)
- Ollama (for LLM features)
- ChromaDB
- API Keys (Claude/Groq) if using external LLMs

### Step-by-Step Installation

#### 1. Install System Dependencies

**Windows:**
```cmd
# Python 3.9+ from Python.org (add to PATH)
# Visual Studio Build Tools (Desktop C++ workload)
# CMake from CMake.org (add to PATH)
# FFmpeg (download or: choco install ffmpeg)
# Git from Git-scm.com
# Ollama from Ollama.com
```

**macOS:**
```bash
# Install Homebrew if not already installed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies
brew install python@3.9 cmake llvm libomp ffmpeg git ollama
```

#### 2. Install Python Dependencies
```bash
# Windows
python -m pip install --upgrade pip
python -m pip install -r requirements.txt

# macOS
python3 -m pip install --upgrade pip
python3 -m pip install -r requirements.txt
```

#### 3. Build Whisper Server
```bash
# Windows
./build_whisper.cmd

# macOS (make executable if needed)
chmod +x build_whisper.sh
./build_whisper.sh
```

#### 4. Start Services
```bash
# Windows
./start_with_output.ps1

# macOS
chmod +x clean_start_backend.sh
./clean_start_backend.sh
```

---

## 📚 API Documentation

Once services are running:
- **Swagger UI**: http://localhost:5167/docs
- **ReDoc**: http://localhost:5167/redoc

### Core Services
1. **Whisper.cpp Server** (Port 8178)
   - Real-time audio transcription
   - WebSocket support for streaming
   - Multiple model support

2. **FastAPI Backend** (Port 5167)
   - Meeting management APIs
   - LLM integration (Claude, Groq, Ollama)
   - Data storage and retrieval
   - WebSocket for real-time updates

---

## 🛠️ Troubleshooting

### Common Docker Issues

**Port Conflicts:**
```bash
# Stop services
./run-docker.sh stop  # or .\run-docker.ps1 stop

# Check port usage
netstat -an | grep :8178
lsof -i :8178  # macOS/Linux
```

**GPU Not Detected (Windows):**
- Enable WSL2 integration in Docker Desktop
- Install nvidia-container-toolkit
- Verify with: `.\run-docker.ps1 gpu-test`

**Model Download Failures:**
```bash
# Manual download
./run-docker.sh models download base.en
# or
.\run-docker.ps1 models download base.en
```

### Common Native Issues

**Windows Build Problems:**
```cmd
# CMake not found - install Visual Studio Build Tools
# PowerShell execution blocked:
Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process
```

**macOS Build Problems:**
```bash
# Compilation errors
brew install cmake llvm libomp
export CC=/opt/homebrew/bin/clang
export CXX=/opt/homebrew/bin/clang++

# Permission denied
chmod +x build_whisper.sh
chmod +x clean_start_backend.sh

# Port conflicts
lsof -i :5167  # Find process using port
kill -9 PID   # Kill process
```

### General Issues

**Services Won't Start:**
1. Check if ports 8178 (Whisper) and 5167 (Backend) are available
2. Verify all dependencies are installed
3. Check logs for specific error messages
4. Ensure sufficient system resources (8GB+ RAM recommended)

**Model Issues:**
- Verify internet connection for model downloads
- Check available disk space (models can be 1.5GB+)
- Validate model names against supported list

---

## 📖 Complete Script Reference

### Docker Scripts

#### build-docker.ps1 / build-docker.sh
Build Docker images with GPU support and cross-platform compatibility.

**Usage:**
```bash
# Build Types
cpu, gpu, macos, both, test-gpu

# Options
-Registry/-r REGISTRY    # Docker registry
-Push/-p                 # Push to registry
-Tag/-t TAG             # Custom tag
-Platforms PLATFORMS    # Target platforms
-BuildArgs ARGS         # Build arguments
-NoCache/--no-cache     # Build without cache
-DryRun/--dry-run       # Show commands only
```

**Examples:**
```bash
# Basic builds
.\build-docker.ps1 cpu
./build-docker.sh gpu

# Multi-platform with registry
.\build-docker.ps1 both -Registry "ghcr.io/user" -Push
./build-docker.sh cpu --platforms "linux/amd64,linux/arm64" --push
```

#### run-docker.ps1 / run-docker.sh
Complete Docker deployment manager with interactive setup.

**Commands:**
```bash
start, stop, restart, logs, status, shell, clean, build, models, gpu-test, setup-db, compose
```

**Start Options:**
```bash
-Model/-m MODEL         # Whisper model (default: base.en)
-Port/-p PORT          # Whisper port (default: 8178)
-AppPort/--app-port    # Meeting app port (default: 5167)
-Gpu/-g/--gpu          # Force GPU mode
-Cpu/-c/--cpu          # Force CPU mode
-Language/--language   # Language code (default: auto)
-Translate/--translate # Enable translation
-Diarize/--diarize     # Enable diarization
-Detach/-d/--detach    # Run in background
-Interactive/-i        # Interactive setup
```

**Examples:**
```bash
# Interactive setup
.\run-docker.ps1 start -Interactive
./run-docker.sh start --interactive

# Advanced configuration
.\run-docker.ps1 start -Model large-v3 -Gpu -Language es -Detach
./run-docker.sh start --model base --translate --diarize --detach

# Management
.\run-docker.ps1 logs -Service whisper -Follow
./run-docker.sh logs --service app --follow --lines 100
```

### Native Scripts

#### build_whisper.cmd / build_whisper.sh
Build whisper.cpp server with custom modifications.

**Usage:**
```bash
build_whisper.cmd [MODEL_NAME]    # Windows
./build_whisper.sh [MODEL_NAME]   # macOS/Linux
```

**Available Models:**
```
tiny, tiny.en, base, base.en, small, small.en, medium, medium.en,
large-v1, large-v2, large-v3, large-v3-turbo, 
*-q5_1 (5-bit quantized), *-q8_0 (8-bit quantized)
```

### Environment Variables

**Service Configuration:**
```bash
WHISPER_MODEL=base.en          # Default model
WHISPER_PORT=8178              # Whisper port
APP_PORT=5167                  # App port
WHISPER_LANGUAGE=auto          # Language
WHISPER_TRANSLATE=false        # Translation
WHISPER_DIARIZE=false          # Diarization
```

**Build Configuration:**
```bash
REGISTRY=ghcr.io/user          # Docker registry
PUSH=true                      # Push to registry
PLATFORMS=linux/amd64          # Target platforms
FORCE_GPU=true                 # Force GPU mode
DEBUG=true                     # Debug output
```

### Database Migration

**Supported Sources:**
- Existing Homebrew installations
- Manual database file paths
- Auto-discovery in common locations
- Fresh installation (creates new database)

**Auto-Discovery Paths (macOS/Linux):**
```
/opt/homebrew/Cellar/meetily-backend/*/backend/meeting_minutes.db
$HOME/.meetily/meeting_minutes.db
$HOME/Documents/meetily/meeting_minutes.db
$HOME/Desktop/meeting_minutes.db
./meeting_minutes.db
$SCRIPT_DIR/data/meeting_minutes.db
```

### Advanced Features

**Port Conflict Resolution:**
- Automatic detection of port conflicts
- Option to kill processes using required ports
- Suggestion of alternative ports
- Validation of port availability

**GPU Detection:**
- Automatic NVIDIA GPU detection
- Docker GPU support verification
- Fallback to CPU mode when GPU unavailable
- GPU test functionality

**Model Management:**
- Automatic model downloading
- Size estimation and progress display
- Local model caching
- Model validation and integrity checking

**Interactive Setup:**
- Model selection with guidance
- Language selection (40+ languages)
- Database migration assistance
- Settings persistence and reuse
- Configuration validation

This comprehensive guide covers all deployment options and provides clear instructions for getting the Meetily backend running in any environment.