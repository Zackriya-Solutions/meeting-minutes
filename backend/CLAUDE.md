# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a **Docker-based distribution system** for the Whisper speech-to-text server that solves cross-platform executable distribution challenges. The project wraps the whisper.cpp server example in Docker containers with automatic GPU detection, model management, and universal compatibility.

## Architecture

### Core Components

**Docker Images:**
- `Dockerfile.server-cpu` - CPU-only build for universal compatibility
- `Dockerfile.server-gpu` - NVIDIA GPU-enabled build with CUDA support
- Both use multi-stage builds (builder + runtime) and include the whisper-server binary from `whisper.cpp/examples/server/`

**Smart Management Scripts:**
- `build-docker.sh` - Multi-platform Docker builder with automatic platform detection
- `run-docker.sh` - Deployment manager with GPU detection and container lifecycle management
- `docker/entrypoint.sh` - Container startup script with GPU detection, model downloads, and server configuration

**Configuration:**
- `docker-compose.yml` - Production-ready orchestration with profiles for different deployment scenarios
- Environment-based configuration for models, GPU settings, and server parameters

### Key Architectural Decisions

**GPU Detection Strategy:** The system uses a two-tier GPU detection approach:
1. `detect_gpu_silent()` - Used for command building without logging interference
2. `detect_gpu()` - Used for user-facing output with detailed hardware information

**Model Management:** Models are automatically downloaded from HuggingFace on first use and cached in persistent volumes. The system supports all whisper.cpp model sizes (tiny to large-v3).

**Build System:** Uses Docker Buildx for cross-platform builds with automatic platform detection for local builds and multi-platform support for registry distribution.

## Common Commands

### Build and Deploy
```bash
# Build CPU-only version (automatic platform detection)
./build-docker.sh cpu

# Build GPU version 
./build-docker.sh gpu

# Build both versions
./build-docker.sh both

# Clean build without cache
./build-docker.sh cpu --no-cache

# Start server with auto-build
./run-docker.sh start --model large-v3 --port 8081

# Start in background
./run-docker.sh start --detach

# Use Docker Compose
docker-compose up
DOCKERFILE=Dockerfile.server-gpu docker-compose up
```

### Management Commands
```bash
# Check server status and logs
./run-docker.sh status
./run-docker.sh logs

# Stop/restart server
./run-docker.sh stop
./run-docker.sh restart

# Test GPU detection
./run-docker.sh gpu-test

# Model management
./run-docker.sh models list
./run-docker.sh models download base.en

# Clean up containers and images
./run-docker.sh clean --images
```

### Distribution
```bash
# Save for offline distribution
docker save whisper-server:cpu | gzip > whisper-server-cpu.tar.gz

# Registry distribution
./build-docker.sh cpu --registry your-name --push --platforms linux/amd64,linux/arm64
```

## Development Guidelines

### Docker Image Modifications
- CPU builds must remain universally compatible
- GPU builds should gracefully fallback to CPU when GPU unavailable
- Always test both ARM64 (Mac) and AMD64 (Linux) builds
- Use multi-stage builds to minimize final image size

### Script Development
- All scripts use consistent logging functions (`log_info`, `log_warn`, `log_error`)
- GPU detection functions must not output logs when used in command building contexts
- Build scripts should auto-detect platform for local builds and support multi-platform for registry builds

### Container Configuration  
- Use environment variables for all runtime configuration
- Models should auto-download if not present
- Server should bind to 0.0.0.0 inside container for port forwarding
- Include health checks and proper signal handling

## Troubleshooting

### Build Issues
- Multi-platform builds without `--push` will fail (Docker limitation)
- Missing whisper.cpp submodule: ensure `WHISPER_BUILD_EXAMPLES=ON` in Dockerfiles
- Image not found errors: run scripts automatically build missing images

### Runtime Issues
- Container exits immediately: check logs with `./run-docker.sh logs`
- Model download failures: ensure internet connectivity and disk space
- GPU not detected: verify nvidia-docker2 installation for NVIDIA GPUs

### Cross-Platform Compatibility
- ARM64 builds tested on Apple Silicon Macs
- AMD64 builds tested on Linux and Windows with Docker Desert
- GPU support currently limited to NVIDIA (CUDA), with CPU fallback always available