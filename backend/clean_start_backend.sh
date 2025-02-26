#!/bin/bash

# Exit on error
set -e

# Color codes and emojis
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

# Configuration
PACKAGE_NAME="whisper-server-package"
MODEL_DIR="$PACKAGE_NAME/models"

# Helper functions for logging
log_info() {
    echo -e "${BLUE}ℹ️  [INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}✅ [SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}⚠️  [WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}❌ [ERROR]${NC} $1"
    return 1
}

log_section() {
    echo -e "\n${PURPLE}🔄 === $1 ===${NC}\n"
}

# Error handling function
handle_error() {
    local error_msg="$1"
    log_error "$error_msg"
    cleanup
    exit 1
}

# Cleanup function
cleanup() {
    log_section "Cleanup"
    if [ -n "$WHISPER_PID" ]; then
        log_info "Stopping Whisper server..."
        if kill -0 $WHISPER_PID 2>/dev/null; then
            kill -9 $WHISPER_PID 2>/dev/null || log_warning "Failed to kill Whisper server process"
            pkill -9 -f "whisper-server" 2>/dev/null || log_warning "Failed to kill remaining whisper-server processes"
        fi
        log_success "Whisper server stopped"
    fi
    if [ -n "$PYTHON_PID" ]; then
        log_info "Stopping Python backend..."
        if kill -0 $PYTHON_PID 2>/dev/null; then
            kill -9 $PYTHON_PID 2>/dev/null || log_warning "Failed to kill Python backend process"
        fi
        log_success "Python backend stopped"
    fi
}

# Set up trap for cleanup on script exit, interrupt, or termination
trap cleanup EXIT INT TERM

# Check if required directories and files exist
log_section "Environment Check"

if [ ! -d "$PACKAGE_NAME" ]; then
    handle_error "Whisper server directory not found. Please run build_whisper.sh first"
fi

if [ ! -d "app" ]; then
    handle_error "Python backend directory not found. Please check your installation"
fi

if [ ! -f "app/main.py" ]; then
    handle_error "Python backend main.py not found. Please check your installation"
fi

if [ ! -d "venv" ]; then
    handle_error "Virtual environment not found. Please run build_whisper.sh first"
fi

# Kill any existing whisper-server processes
log_section "Initial Cleanup"

log_info "Checking for existing whisper servers..."
if pkill -f "whisper-server" 2>/dev/null; then
    log_success "Existing whisper servers terminated"
else
    log_warning "No existing whisper servers found"
fi
sleep 1  # Give processes time to terminate

# Check and kill if backend app in port 5167 is running
log_section "Backend App Check"

log_info "Checking for processes on port 5167..."
PORT=5167
if lsof -i :$PORT | grep -q LISTEN; then
    log_warning "Backend app is running on port $PORT"
    read -p "$(echo -e "${YELLOW}🤔 Kill it? (y/N)${NC} ")" -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        handle_error "User chose not to terminate existing backend app"
    fi

    log_info "Terminating backend app..."
    if ! kill -9 $(lsof -t -i :$PORT) 2>/dev/null; then
        handle_error "Failed to terminate backend app"
    fi
    log_success "Backend app terminated"
    sleep 1  # Give processes time to terminate
fi



# Check for existing model
log_section "Model Check"

if [ ! -d "$MODEL_DIR" ]; then
    handle_error "Models directory not found. Please run build_whisper.sh first"
fi

log_info "Checking for Whisper models..."
EXISTING_MODELS=$(find "$MODEL_DIR" -name "ggml-*.bin" -type f)

if [ -n "$EXISTING_MODELS" ]; then
    log_success "Found existing models:"
    echo -e "${BLUE}$EXISTING_MODELS${NC}"
else
    log_warning "No existing models found"
fi

# Whisper models
models="tiny
tiny.en
tiny-q5_1
base
base.en
base-q5_1
small
small.en
small-q5_1
medium
medium.en
medium-q5_1
large-v1
large-v2
large-v3
large-v1-q5_1
large-v2-q5_1
large-v3-q5_1
large-v1-turbo
large-v2-turbo
large-v3-turbo
large-v1-turbo-q5_0
large-v2-turbo-q5_0
large-v3-turbo-q5_0
large-v1-turbo-q8_0
large-v2-turbo-q8_0
large-v3-turbo-q8_0"

# Ask user which model to use if the argument is not provided
if [ -z "$1" ]; then
    log_section "Model Selection"
    log_info "Available models:"
    echo -e "${BLUE}$models${NC}"
    read -p "$(echo -e "${YELLOW}🎯 Enter a model name (e.g. small):${NC} ")" MODEL_SHORT_NAME
else
    MODEL_SHORT_NAME=$1
fi

# Check if the model is valid
if ! echo "$models" | grep -qw "$MODEL_SHORT_NAME"; then
    handle_error "Invalid model: $MODEL_SHORT_NAME"
fi

MODEL_NAME="ggml-$MODEL_SHORT_NAME.bin"
log_success "Selected model: $MODEL_NAME"

# Check if the modelname exists in directory
if [ -f "$MODEL_DIR/$MODEL_NAME" ]; then
    log_success "Model file exists: $MODEL_DIR/$MODEL_NAME"
else
    log_warning "Model file does not exist: $MODEL_DIR/$MODEL_NAME"
    log_info "Downloading model... 📥"
    if ! ./download-ggml-model.sh $MODEL_SHORT_NAME; then
        handle_error "Failed to download model"
    fi

    # Move model to models directory
    mv "$MODEL_NAME" "$MODEL_DIR/" || handle_error "Failed to move model to models directory"
fi

log_section "Starting Services"

# Start the whisper server in background
log_info "Starting Whisper server... 🎙️"
cd "$PACKAGE_NAME" || handle_error "Failed to change to whisper-server directory"
./run-server.sh --model "models/$MODEL_NAME" &
WHISPER_PID=$!
cd .. || handle_error "Failed to return to root directory"

# Wait for server to start and check if it's running
sleep 2
if ! kill -0 $WHISPER_PID 2>/dev/null; then
    handle_error "Whisper server failed to start"
fi

# Start the Python backend in background
log_info "Starting Python backend... 🚀"
# Start venv if not active
if [ -z "$VIRTUAL_ENV" ]; then
    log_info "Activating virtual environment..."
    if ! source venv/bin/activate; then
        handle_error "Failed to activate virtual environment"
    fi
fi

# Check if required Python packages are installed
if ! pip show fastapi >/dev/null 2>&1; then
    handle_error "FastAPI not found. Please run build_whisper.sh to install dependencies"
fi

python app/main.py &
PYTHON_PID=$!

# Wait for backend to start and check if it's running
sleep 10
if ! kill -0 $PYTHON_PID 2>/dev/null; then
    handle_error "Python backend failed to start"
fi

# Check if the port is actually listening
if ! lsof -i :$PORT | grep -q LISTEN; then
    handle_error "Python backend is not listening on port $PORT"
fi

log_success "🎉 All services started successfully!"
echo -e "${GREEN}🔍 Whisper Server (PID: $WHISPER_PID)${NC}"
echo -e "${GREEN}🐍 Python Backend (PID: $PYTHON_PID)${NC}"
echo -e "${BLUE}Press Ctrl+C to stop all services${NC}"

# Show whisper server port and python backend port
echo -e "${BLUE}Whisper Server Port: $PORT${NC}"
echo -e "${BLUE}Python Backend Port: 8178${NC}"

# Keep the script running and wait for both processes
wait $WHISPER_PID $PYTHON_PID || handle_error "One of the services crashed"
