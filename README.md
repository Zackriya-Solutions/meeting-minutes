<div align="center" style="border-bottom: none">
    <h1>
        <img src="docs/Meetily-6.png" style="border-radius: 10px;" />
        <br>
        Privacy-First AI Meeting Assistant
    </h1>
    <a href="https://trendshift.io/repositories/13272" target="_blank"><img src="https://trendshift.io/api/badge/repositories/13272" alt="Zackriya-Solutions%2Fmeeting-minutes | Trendshift" style="width: 250px; height: 55px;" width="250" height="55"/></a>
    <br>
    <br>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases/"><img src="https://img.shields.io/badge/Pre_Release-Link-brightgreen" alt="Pre-Release"></a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img alt="GitHub Repo stars" src="https://img.shields.io/github/stars/zackriya-solutions/meeting-minutes?style=flat">
</a>
 <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"> <img alt="GitHub Downloads (all assets, all releases)" src="https://img.shields.io/github/downloads/zackriya-solutions/meeting-minutes/total?style=plastic"> </a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img src="https://img.shields.io/badge/License-MIT-blue" alt="License"></a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img src="https://img.shields.io/badge/Supported_OS-macOS,_Windows-white" alt="Supported OS"></a>
    <a href="https://github.com/Zackriya-Solutions/meeting-minutes/releases"><img alt="GitHub Tag" src="https://img.shields.io/github/v/tag/zackriya-solutions/meeting-minutes?include_prereleases&color=yellow">
</a>
    <br>
    <h3>
    <br>
    Open Source • Privacy-First • Enterprise-Ready
    </h3>
    <p align="center">
    Get latest <a href="https://www.zackriya.com/meetily-subscribe/"><b>Product updates</b></a> <br><br>
    <a href="https://meetily.zackriya.com"><b>Website</b></a> •
    <a href="https://www.linkedin.com/company/106363062/"><b>LinkedIn</b></a> •
    <a href="https://discord.gg/crRymMQBFH"><b>Meetily Discord</b></a> •
    <a href="https://discord.com/invite/vCFJvN4BwJ"><b>Privacy-First AI</b></a> •
    <a href="https://www.reddit.com/r/meetily/"><b>Reddit</b></a>
</p>
    <p align="center">
    
 A privacy-first AI meeting assistant that captures, transcribes, and summarizes meetings entirely on your infrastructure. Built by expert AI engineers passionate about data sovereignty and open source solutions. Perfect for enterprises that need advanced meeting intelligence without compromising on privacy, compliance, or control. 
</p>

<p align="center">
    <img src="docs/demo_small.gif" width="650" alt="Meetily Demo" />
    <br>
    <a href="https://youtu.be/5k_Q5Wlahuk">View full Demo Video</a>
</p>

</div>

## For enterprise version: [Sign up for early access](https://meetily.zackriya.com/#pricing)

## For Partnerships and Custom AI development: [Let's chat](https://www.zackriya.com/service-interest-form/)

# Table of Contents
- [Overview](#overview)
- [The Privacy Problem](#the-privacy-problem)
- [Features](#features)
- [System Architecture](#system-architecture)
  - [Core Components](#core-components)
  - [Deployment Architecture](#deployment-architecture)
- [Quick Start Guide](#quick-start-guide)
- [Prerequisites](#prerequisites)
- [Setup Instructions](#setup-instructions)
  - [Windows Setup](#windows-setup)
    - [Frontend Setup](#frontend-setup)
    - [Backend Setup](#backend-setup)
  - [macOS Setup](#macos-setup)
    - [Frontend Setup](#frontend-setup-1)
    - [Backend Setup](#backend-setup-1)
  - [Docker Setup (Alternative)](#docker-setup-alternative)
- [Whisper Model Selection](#whisper-model-selection)
- [LLM Integration](#llm-integration)
  - [Supported Providers](#supported-providers)
- [Troubleshooting](#troubleshooting)
  - [Docker Issues](#docker-issues)
  - [Native Installation Issues](#native-installation-issues)
  - [General Issues](#general-issues)
- [Developer Console](#developer-console)
- [Uninstallation](#uninstallation)
- [Enterprise Solutions](#enterprise-solutions)
- [Partnerships & Referrals](#partnerships--referrals)
- [Development Guidelines](#development-guidelines)
- [Contributing](#contributing)
- [License](#license)
- [About Our Team](#about-our-team)
- [Acknowledgments](#acknowledgments)
- [Star History](#star-history)

# Overview

A privacy-first AI meeting assistant that captures, transcribes, and summarizes meetings entirely on your infrastructure. Built by expert AI engineers passionate about data sovereignty and open source solutions. Perfect for professionals and enterprises that need advanced meeting intelligence without compromising privacy or control.

### Why?

While there are many meeting transcription tools available, this solution stands out by offering:
- **Privacy First**: All processing happens locally on your device
- **Cost Effective**: Uses open-source AI models instead of expensive APIs
- **Flexible**: Works offline, supports multiple meeting platforms
- **Customizable**: Self-host and modify for your specific needs
- **Intelligent**: Built-in knowledge graph for semantic search across meetings



## The Privacy Problem

Meeting AI tools create significant privacy and compliance risks across all sectors:
- **$4.4M average cost per data breach** (IBM 2024)
- **€5.88 billion in GDPR fines** issued by 2025
- **400+ unlawful recording cases** filed in California this year

Whether you're a defense consultant, enterprise executive, legal professional, or healthcare provider, your sensitive discussions shouldn't live on servers you don't control. Cloud meeting tools promise convenience but deliver privacy nightmares with unclear data storage practices and potential unauthorized access.

**Meetily solves this**: Complete data sovereignty on your infrastructure, zero vendor lock-in, full control over your sensitive conversations.

# Features

✅ Modern, responsive UI with real-time updates

✅ Real-time audio capture (microphone + system audio)

✅ Live transcription using locally-running Whisper

✅ Local processing for privacy

✅ Packaged the app for macOS and Windows

✅ Rich text editor for notes

🚧 Export to Markdown/PDF/HTML

🚧 Obsidian Integration 

🚧 Speaker diarization

---

# Quick Start Guide

Choose your setup method based on your needs:

## 🚀 Option 1: Native Installation (Recommended for Best Performance)
**Best for:** Regular users wanting optimal performance  
**Time:** 10-15 minutes  
**System Requirements:** 8GB+ RAM, 4GB+ disk space

### Windows Users:
1. **Frontend:** Download and run [meetily-frontend_0.0.5_x64-setup.exe](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
2. **Backend:** Follow [Windows Backend Setup](#backend-setup)

For safety and to maintain proper user permissions for frontend app:

1. Go to [Latest Releases](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
2. Download the file ending with `x64-setup.exe`
3. **Important:** Before running, right-click the file → **Properties** → Check **Unblock** at bottom → **OK**


<p align="center">
    <img src="https://github.com/user-attachments/assets/f2a2655d-9881-42ed-88aa-357a1f5b6118" width="300" alt="Windows Security Warning" />
</p>

4. Double-click the installer to run it
5. If Windows shows a security warning:
   - Click `More info` and choose `Run anyway`, or
   - Follow the permission dialog prompts
6. Follow the installation wizard

✅ **Success Check:** You should see the Meetily application window open successfully when launched.



### macOS Users:
1. **Complete Setup (Recommended):**
   ```bash
   # Install both frontend + backend
   brew tap zackriya-solutions/meetily
   brew install --cask meetily
   
   # Start the backend server
   meetily-server --language en --model medium
   ```
2. Open **Meetily** from Applications folder

## 🐳 Option 2: Docker Setup (Easier but Slower)
**Best for:** Developers, quick testing, or multi-environment deployment  
**Time:** 5-10 minutes  
**System Requirements:** 16GB+ RAM (8GB minimum for Docker), Docker Desktop

⚠️ **Performance Note:** Docker setup is 20-30% slower than native installation but offers easier dependency management.

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

## ✅ Quick Success Check
After setup, verify everything works:
1. **Whisper Server:** Visit http://localhost:8178 (should show API interface)
2. **Backend API:** Visit http://localhost:5167/docs (should show API documentation)
3. **Frontend App:** Open Meetily application and test microphone access

### 🚨 Common Issues
- **Windows Defender blocking installer?** → See [Windows Defender Troubleshooting](#windows-defender-issues) below
- **Can't access localhost:8178 or 5167?** → Check if backend is running and ports are available
- **"Permission denied" errors?** → Run `chmod +x` on script files (macOS/Linux) or check execution policy (Windows)
- **Docker containers crashing?** → Increase Docker RAM allocation to 12GB+ and check available disk space
- **Audio not working?** → Grant microphone permissions to the app in system settings

👉 **For detailed troubleshooting, see [Troubleshooting Section](#troubleshooting)**

---

# System Architecture

<p align="center">
    <img src="docs/HighLevel.jpg" width="900" alt="Meetily High Level Architecture" />
</p>

### Core Components

1. **Audio Capture Service**
   - Real-time microphone/system audio capture
   - Audio preprocessing pipeline
   - Built with Rust (experimental) and Python

2. **Transcription Engine**
   - Whisper.cpp for local transcription
   - Supports multiple model sizes (tiny->large)
   - GPU-accelerated processing

3. **LLM Orchestrator**
   - Unified interface for multiple providers
   - Automatic fallback handling
   - Chunk processing with overlap
   - Model configuration:

4. **Data Services**
   - **ChromaDB**: Vector store for transcript embeddings
   - **SQLite**: Process tracking and metadata storage


### Deployment Architecture

- **Frontend**: Tauri app + Next.js (packaged executables)
- **Backend**: Python FastAPI:
  - Transcript workers
  - LLM inference

# Prerequisites

## 💻 System Requirements

### Minimum Requirements
- **RAM:** 8GB (16GB+ recommended)
- **Storage:** 4GB free space
- **CPU:** 4+ cores
- **OS:** Windows 10/11, macOS 10.15+, or Ubuntu 18.04+

### Recommended Requirements  
- **RAM:** 16GB+ (for large Whisper models)
- **Storage:** 10GB+ free space
- **CPU:** 8+ cores or Apple Silicon Mac
- **GPU:** NVIDIA GPU with CUDA (optional, for faster processing)

## 🛠️ Required Software

### For Native Installation
| Component | Windows | macOS | Purpose |
|-----------|---------|-------|---------|
| **Python** | 3.9+ ([python.org](https://python.org)) | `brew install python` | Backend runtime |
| **Node.js** | 18+ LTS ([nodejs.org](https://nodejs.org)) | `brew install node` | Frontend build |
| **Git** | ([git-scm.com](https://git-scm.com)) | Pre-installed | Code download |
| **FFmpeg** | `winget install FFmpeg` | `brew install ffmpeg` | Audio processing |

### For Docker Installation  
- **Docker Desktop** ([docker.com](https://docker.com))
- **16GB+ RAM** allocated to Docker
- **4+ CPU cores** allocated to Docker

## 🔧 Build Tools (Native Installation Only)

### Windows
```powershell
# Install Visual Studio Build Tools (required for Whisper.cpp compilation)
# Download from: https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2019
```

### macOS  
```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install Homebrew (if not already installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

### Linux (Ubuntu/Debian)
```bash
sudo apt-get update
sudo apt-get install build-essential cmake git ffmpeg python3 python3-pip nodejs npm
```

## 🤖 Optional LLM Integration
- **Ollama** ([ollama.com](https://ollama.com)) - For local AI models
- **API Keys** - For Claude (Anthropic) or Groq services


# Setup Instructions

## Windows Setup

⏱️ **Estimated Time:** 10-15 minutes total

### Frontend Setup
⏱️ **Time:** ~3-5 minutes

**Manual Download (Recommended)**

For safety and to maintain proper user permissions:

1. Go to [Latest Releases](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
2. Download the file ending with `x64-setup.exe`
3. **Important:** Before running, right-click the file → **Properties** → Check **Unblock** at bottom → **OK**
4. Double-click the installer to run it
5. If Windows shows a security warning:
   - Click `More info` and choose `Run anyway`, or
   - Follow the permission dialog prompts

<p align="center">
    <img src="https://github.com/user-attachments/assets/f2a2655d-9881-42ed-88aa-357a1f5b6118" width="300" alt="Windows Security Warning" />
</p>

6. Follow the installation wizard
7. The application will be available on your desktop

✅ **Success Check:** You should see the Meetily application window open successfully when launched.

**Alternative: MSI Installer (Less likely to be blocked)**

1. Go to [Latest Releases](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
2. Download the file ending with `x64_en-US.msi`
3. Double-click the MSI file to run it
4. Follow the installation wizard to complete the setup
5. The application will be installed and available on your desktop

Provide necessary permissions for audio capture and microphone access.

### Backend Setup
⏱️ **Time:** ~10-15 minutes (includes dependency installation)

Click on the image to see installation video

<p align="center">
<a href="https://www.youtube.com/watch?v=BeH7B-2nPMk">
<img src="https://img.youtube.com/vi/BeH7B-2nPMk/0.jpg" alt="Windows Backend Setup Tutorial" />
</a>
</p>

**Option 1: Manual Setup (Recommended)**

**Step 1: Install Dependencies (Optional)**

Open PowerShell as administrator and enter the following command to install dependencies:

```powershell
cd ~/Downloads
git clone https://github.com/Zackriya-Solutions/meeting-minutes
cd meeting-minutes/backend
Set-ExecutionPolicy Bypass -Scope Process -Force
.\install_dependancies_for_windows.ps1
```

**⚠️ This can take up to 30 minutes depending on your system**

Once the installation is complete, close the PowerShell terminal and open a new terminal.

**Step 2: Build Whisper**

Enter the following commands to build the backend:

```cmd
cd meeting-minutes/backend
.\build_whisper.cmd
```

If the build fails, run the command again:

```cmd
.\build_whisper.cmd
```

**Step 3: Start the Backend**

Finally, when the installation is successful, run the backend using:

```powershell
.\start_with_output.ps1
```

✅ **Success Check:** You should see both Whisper server (port 8178) and Meeting app (port 5167) start successfully with log messages indicating they're running.

#### Troubleshooting

1. **Warning - existing chocolatey installation is detected**

   ![WhatsApp Image 2025-05-27 at 12 31 17 PM](https://github.com/user-attachments/assets/e2839c25-33e0-4972-808a-dd00c8bc568a)

   To address this - Either use the current chocolatey version installed or remove the current one with:
   ```powershell
   rm C:\ProgramData\chocolatey
   ```

2. **Error - ./start_with_output.ps1 shows security error**

   Run after making sure the file is unblocked:
   ```powershell
   Set-ExecutionPolicy Bypass -Scope Process -Force
   .\start_with_output.ps1
   ```


## Docker Setup (Alternative)

⚠️ **Performance Warning**: Docker setup is 20-30% slower than native installation but offers easier dependency management and consistent environments across different systems.

### Prerequisites
- Docker Desktop (Windows/Mac) or Docker Engine (Linux)  
- 16GB+ RAM (8GB minimum allocated to Docker)
- 4+ CPU cores recommended
- For GPU: NVIDIA drivers + nvidia-container-toolkit (Windows/Linux only)

### Quick Start

#### Windows (PowerShell)
```powershell
# Navigate to backend directory
cd backend

# Build and start services
.\build-docker.ps1 cpu                     # Build CPU version
.\run-docker.ps1 start -Interactive        # Interactive setup (recommended)
```

#### macOS/Linux (Bash)
```bash
# Navigate to backend directory  
cd backend

# Build and start services
./build-docker.sh cpu                      # Build CPU version
./run-docker.sh start --interactive        # Interactive setup (recommended)
```

### After Startup
- **Whisper Server**: http://localhost:8178
- **Meeting App**: http://localhost:5167 (with API docs at `/docs`)

### Advanced Options
```bash
# GPU acceleration (Windows/Linux only)
.\build-docker.ps1 gpu  # Windows
./build-docker.sh gpu   # Linux

# Custom configuration
.\run-docker.ps1 start -Model large-v3 -Language es -Detach
./run-docker.sh start --model large-v3 --language es --detach
```

### Management Commands
```bash
# Check status and logs
.\run-docker.ps1 status                   # Windows
./run-docker.sh status                    # macOS/Linux

# Stop services
.\run-docker.ps1 stop                     # Windows  
./run-docker.sh stop                      # macOS/Linux
```

## macOS Setup

⏱️ **Estimated Time:** 5-10 minutes total

**Option 1: Using Homebrew (Recommended) - Complete Setup**
⏱️ **Time:** ~5-7 minutes

> **Note**: This single command installs both the frontend app and backend server.

```bash
# Install Meetily (frontend + backend)
brew tap zackriya-solutions/meetily
brew install --cask meetily

# Start the backend server
meetily-server --language en --model medium
```

**How to use after installation:**
1. Run `meetily-server` in terminal (keep it running)
2. Open **Meetily** from Applications folder or Spotlight
3. Grant microphone and screen recording permissions when prompted

✅ **Success Check:** Meetily app should open and you should be able to start recording meetings immediately.

**To update existing installation:**
```bash
# Update to latest version
brew upgrade --cask meetily
```

**Option 2: Manual Installation**
⏱️ **Time:** ~8-12 minutes

1. Download the latest [dmg_darwin_arch64.zip](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest) file
2. Extract the file
3. Double-click the `.dmg` file inside the extracted folder
4. Drag the application to your Applications folder
5. Remove quarantine attribute:
   ```bash
   xattr -c /Applications/meetily-frontend.app
   ```
6. Grant necessary permissions for audio capture and microphone access
7. **Important**: You'll need to install the backend separately (see Manual Backend Setup below)

### Backend Setup

**Option 1: Using Homebrew Backend Only**
⏱️ **Time:** ~3-5 minutes
```bash
# Install just the backend (if you manually installed frontend)
brew tap zackriya-solutions/meetily  
brew install meetily-backend

# Start the backend server
meetily-server --language en --model medium
```

**Option 2: Complete Manual Setup**
⏱️ **Time:** ~10-15 minutes
```bash
# Clone the repository
git clone https://github.com/Zackriya-Solutions/meeting-minutes.git
cd meeting-minutes/backend

# Create and activate virtual environment
python -m venv venv
source venv/bin/activate

# Install dependencies
pip install -r requirements.txt


# Build dependencies
chmod +x build_whisper.sh
./build_whisper.sh

# Start backend servers
./clean_start_backend.sh
```


### Development Setup

```bash
# Navigate to frontend directory
cd frontend

# Give execute permissions to clean_build.sh
chmod +x clean_build.sh

# run clean_build.sh
./clean_build.sh
```

### Whisper Model Selection

When setting up the backend (either via Homebrew, manual installation, or Docker), you can choose from various Whisper models based on your needs:

#### Model Size Guide

| Model | Size | Accuracy | Speed | Best For |
|-------|------|----------|-------|----------|
| tiny | ~39 MB | Basic | Fastest | Testing, low resources |
| base | ~142 MB | Good | Fast | General use (recommended) |
| small | ~244 MB | Better | Medium | Better accuracy needed |
| medium | ~769 MB | High | Slow | High accuracy requirements |
| large-v3 | ~1550 MB | Best | Slowest | Maximum accuracy |

#### Recommended Models by RAM

**macOS (Metal acceleration):**
- 8 GB RAM: small
- 16 GB RAM: medium
- 32 GB+ RAM: large-v3

**Windows/Linux:**
- 8 GB RAM: base or small
- 16 GB RAM: medium
- 32 GB+ RAM: large-v3

#### Available Models

1. **Standard models** (balance of accuracy and speed):
   - tiny, base, small, medium, large-v1, large-v2, large-v3, large-v3-turbo

2. **English-optimized models** (faster for English content):
   - tiny.en, base.en, small.en, medium.en

3. **Quantized models** (reduced size, slightly lower quality):
   - *-q5_1 (5-bit quantized), *-q8_0 (8-bit quantized)
   - Example: tiny-q5_1, base-q5_1, small-q5_1, medium-q5_0

**Recommendation:** Start with `base` model for general use, or `base.en` if you're only transcribing English content.


### Known Issues

#### Common Issues
- Smaller LLMs can hallucinate, making summarization quality poor; Please use model above 32B parameter size
- Backend build process requires CMake, C++ compiler, etc. Making it harder to build
- Backend build process requires Python 3.10 or newer
- Frontend build process requires Node.js

#### GPU Support Solution

For those interested in using GPU for faster Whisper inference:

**Windows/Linux GPU Setup:**

1. **Modify build_whisper.cmd:**
   - Locate line 55 in the build_whisper.cmd file
   - Replace it with:
   ```cmd
   cmake .. -DBUILD_SHARED_LIBS=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=ON -DGGML_CUDA=1
   ```

2. **Clean Rebuild Requirement:**
   - If you have previously compiled whisper.cpp for CPU inference, a clean rebuild is essential
   - Create a new directory, git clone meetily into this new folder, then execute the build script
   - This ensures all components are compiled with GPU support from scratch

3. **CUDA Toolkit Installation:**
   - Verify that the CUDA Toolkit is correctly installed on your system
   - This toolkit provides the necessary libraries and tools for CUDA development

4. **Troubleshooting CMake Errors:**
   - If errors persist, refer to [this Stack Overflow post](https://stackoverflow.com/questions/your-specific-issue)
   - Copy required files to Visual Studio folder if needed

For detailed GPU support discussion, see [Issue #126](https://github.com/Zackriya-Solutions/meeting-minutes/issues/126)

## LLM Integration

The backend supports multiple LLM providers through a unified interface. Current implementations include:

### Supported Providers
- **Anthropic** (Claude models)
- **Groq** (Llama3.2 90 B)
- **Ollama** (Local models that supports function calling)

# Troubleshooting

Common issues and solutions organized by setup method:

## Docker Issues

#### Port Conflicts
```bash
# Stop services
./run-docker.sh stop  # or .\run-docker.ps1 stop

# Check port usage
netstat -an | grep :8178
lsof -i :8178  # macOS/Linux
```

#### GPU Not Detected (Windows)
- Enable WSL2 integration in Docker Desktop
- Install nvidia-container-toolkit
- Verify with: `.\run-docker.ps1 gpu-test`

#### Model Download Failures
```bash
# Manual download
./run-docker.sh models download base.en
# or
.\run-docker.ps1 models download base.en
```

#### Audio Processing Issues
If you see "Dropped old audio chunk X due to queue overflow" messages:

1. **Increase Docker Resources** (most important):
   - Memory: 8GB minimum (12GB+ recommended)
   - CPUs: 4+ cores recommended
   - Disk: 20GB+ available space

2. **Use smaller Whisper model**:
   ```bash
   ./run-docker.sh start --model base --detach
   ```

3. **Check container resource usage**:
   ```bash
   docker stats
   ```

### Native Installation Issues

### Windows Defender Issues

If Windows Defender or antivirus software blocks the installer with "virus or potentially unwanted software" error:

#### Option 1: Manual Unblock (Safest Method)
1. Download the installer from [Latest Releases](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
2. Right-click the downloaded `.exe` file → **Properties**
3. Check the **Unblock** checkbox at the bottom → **OK**
4. Double-click the installer to run it
5. Follow the installation prompts

#### Option 2: Windows Security Override
1. Open **Windows Security** → **Virus & threat protection**
2. Under **Virus & threat protection settings**, click **Manage settings**
3. Scroll to **Exclusions** and click **Add or remove exclusions**
4. Add the downloaded installer file as an exclusion
5. Run the installer manually

#### Option 3: Alternative Installation
If Windows Defender continues to block:
1. Use the MSI installer instead (often less flagged): Download `*x64_en-US.msi` from [releases](https://github.com/Zackriya-Solutions/meeting-minutes/releases/latest)
2. Or use manual backend installation only and access via web browser at http://localhost:5167

**Why this happens:** New software releases may trigger false positives in antivirus software until they build trust/reputation.

#### Windows Build Problems
```cmd
# CMake not found - install Visual Studio Build Tools
# PowerShell execution blocked:
Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process
```

#### macOS Build Problems
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

#### Services Won't Start
1. Check if ports 8178 (Whisper) and 5167 (Backend) are available
2. Verify all dependencies are installed
3. Check logs for specific error messages
4. Ensure sufficient system resources (8GB+ RAM recommended)

#### Model Problems

If you encounter issues with the Whisper model:

```bash
# Try a different model size
meetily-download-model small

# Verify model installation
ls -la $(brew --prefix)/opt/meetily-backend/backend/whisper-server-package/models/
```

#### Server Connection Issues

If the server fails to start:

1. Check if ports 8178 and 5167 are available:
   ```bash
   lsof -i :8178
   lsof -i :5167
   ```

2. Verify that FFmpeg is installed correctly:
   ```bash
   which ffmpeg
   ffmpeg -version
   ```

3. Check the logs for specific error messages when running `meetily-server`

4. Try running the Whisper server manually:
   ```bash
   cd $(brew --prefix)/opt/meetily-backend/backend/whisper-server-package/
   ./run-server.sh --model models/ggml-medium.bin
   ```

### Frontend Issues

If the frontend application doesn't connect to the backend:

1. Ensure the backend server is running (`meetily-server`)
2. Check if the application can access localhost:5167
3. Restart the application after starting the backend

If the application fails to launch:

```bash
# Clear quarantine attributes
xattr -cr /Applications/meetily-frontend.app
```

## Docker Script Reference

⚠️ **Performance Note**: While Docker provides easy setup, it has performance limitations compared to native installation. See platform-specific sections above for performance considerations.

### build-docker.ps1 / build-docker.sh
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

### run-docker.ps1 / run-docker.sh
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

**Service URLs:**
- **Whisper Server**: http://localhost:8178 (transcription service)
- **Meeting App**: http://localhost:5167 (AI-powered meeting management)
- **API Documentation**: http://localhost:5167/docs

## Developer Console

The developer console provides real-time logging and debugging information for Meetily. It's particularly useful for troubleshooting issues and monitoring application behavior.

### Accessing the Console

#### Option 1: Development Mode (Recommended for Developers)
When running in development mode, the console is always visible:
```bash
pnpm tauri dev
```
All logs appear in the terminal where you run this command.

#### Option 2: Production Build with UI Toggle
1. Navigate to **Settings** in the app
2. Scroll to the **Developer** section
3. Use the **Developer Console** toggle to show/hide the console
   - **Windows**: Controls the console window visibility
   - **macOS**: Opens Terminal with filtered app logs

#### Option 3: Command Line Access

**macOS:**
```bash
# View live logs
log stream --process meetily-frontend --level info --style compact

# View historical logs (last hour)
log show --process meetily-frontend --last 1h
```

**Windows:**
```bash
# Run the executable directly to see console output
./target/release/meetily-frontend.exe
```

### Console Information

The console displays:
- Application startup and initialization logs
- Recording start/stop events
- Real-time transcription progress
- API connection status
- Error messages and stack traces
- Debug information (when `RUST_LOG=info` is set)

### Use Cases

The console is helpful for:
- **Debugging audio issues**: See which audio devices are detected and used
- **Monitoring transcription**: Track progress and identify bottlenecks
- **Troubleshooting connectivity**: Verify API endpoints and connection status
- **Performance analysis**: Monitor resource usage and processing times
- **Error diagnosis**: Get detailed error messages and context

### Console Window Behavior

**Windows:**
- In release builds, the console window is hidden by default
- Use the UI toggle or run from terminal to see console output
- Console can be shown/hidden at runtime without restarting

**macOS:**
- Uses the system's unified logging
- Console opens in Terminal.app with filtered logs
- Logs persist in the system and can be viewed later

## Uninstallation

To completely remove Meetily:

```bash
# Remove the frontend
brew uninstall --cask meetily

# Remove the backend
brew uninstall meetily-backend

# Optional: remove the taps
brew untap zackriya-solutions/meetily
brew untap zackriya-solutions/meetily-backend

# Optional: remove Ollama if no longer needed
brew uninstall ollama
```


# About Our Team

We are a team of expert AI engineers building privacy-first AI applications and agents. With experience across 20+ product development projects, we understand the critical importance of protecting privacy while delivering cutting-edge AI solutions.

**Our Mission**: Build comprehensive privacy-first AI applications that enterprises and professionals can trust with their most sensitive data.

**Our Values**:
- **Privacy First**: Data sovereignty should never be compromised
- **Open Source**: Transparency and community-driven development
- **Enterprise Ready**: Solutions that scale and meet compliance requirements

Meetily represents the beginning of our vision - a full ecosystem of privacy-first AI tools ranging from meeting assistants to compliance report generators, auditing systems, case research assistants, patent agents, HR automation, and more.

## Enterprise Solutions

**Meetily Enterprise** is available for on-premise deployment, giving organizations complete control over their meeting intelligence infrastructure. This enterprise version includes:

- **100% On-Premise Deployment**: Your data never leaves your infrastructure
- **Centralized Management**: Support for 100+ users with administrative controls  
- **Zero Vendor Lock-in**: Open source MIT license ensures complete ownership
- **Compliance Ready**: Meet GDPR, SOX, HIPAA, and industry-specific requirements
- **Custom Integration**: APIs and webhooks for enterprise systems

For enterprise solutions: [https://meetily.zackriya.com](https://meetily.zackriya.com)

## Partnerships & Referrals

**Help us grow the privacy-first AI ecosystem!** 

We're looking for partners and referrals for early adopters of privacy-first AI solutions:

**Target Industries & Use Cases**:
- Meeting note takers and transcription services
- Compliance report generators
- Auditing support systems  
- Case research assistants
- Patent agents and IP professionals
- HR automation and talent management
- Legal document processing
- Healthcare documentation

**How You Can Help**:
- Refer clients who need privacy-first AI solutions
- Partner with us on custom AI application development
- Collaborate on revenue sharing opportunities
- Get early access to new privacy-first AI tools

Your referrals keep us in business and help us build the future of privacy-first AI. We believe in partnerships that benefit everyone.

For partnerships and custom AI development: [https://www.zackriya.com/service-interest-form/](https://www.zackriya.com/service-interest-form/)

---



## Development Guidelines

- Follow the established project structure
- Write tests for new features
- Document API changes
- Use type hints in Python code
- Follow ESLint configuration for JavaScript/TypeScript

## Contributing

1. Fork the repository
2. Create a feature branch
3. Submit a pull request

## License

MIT License - Feel free to use this project for your own purposes.


## Contributions

Thanks for all the contributions. Our community is what makes this project possible. Below is the list of contributors:

<a href="https://github.com/zackriya-solutions/meeting-minutes/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=zackriya-solutions/meeting-minutes" />
</a>


We welcome contributions from the community! If you have any questions or suggestions, please open an issue or submit a pull request. Please follow the established project structure and guidelines. For more details, refer to the [CONTRIBUTING](CONTRIBUTING.md) file.

## Acknowledgments

- We borrowes some code from [Whisper.cpp](https://github.com/ggerganov/whisper.cpp)
- We borrowes some code from [Screenpipe](https://github.com/mediar-ai/screenpipe)


## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=Zackriya-Solutions/meeting-minutes&type=Date)](https://star-history.com/#Zackriya-Solutions/meeting-minutes&Date)
