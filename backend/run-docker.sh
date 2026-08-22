#!/bin/bash

# Easy deployment script for Whisper Server and Meeting App Docker containers
# Handles model downloads, GPU detection, and container management
#
# ⚠️  AUDIO PROCESSING WARNING:
# Insufficient Docker resources cause audio drops! The audio processing system
# drops chunks when queue is full (MAX_AUDIO_QUEUE_SIZE=10, lib.rs:54).
# Symptoms: "Dropped old audio chunk" in logs (lib.rs:330-333).
# Solution: Allocate 8GB+ RAM and adequate CPU to Docker containers.

set -e

# Configuration
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WHISPER_PROJECT_NAME="whisper-server"
WHISPER_CONTAINER_NAME="whisper-server"
APP_PROJECT_NAME="meeting-app"
APP_CONTAINER_NAME="meeting-app"
DEFAULT_PORT=8178
DEFAULT_APP_PORT=5167
DEFAULT_MODEL="base.en"
PREFERENCES_FILE="$SCRIPT_DIR/.docker-preferences"

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to run docker compose with the correct command
docker_compose() {
    if command -v docker-compose >/dev/null 2>&1; then
        docker-compose "$@"
    elif docker compose version >/dev/null 2>&1; then
        docker compose "$@"
    else
        log_error "Neither 'docker-compose' nor 'docker compose' command found"
        return 1
    fi
}

# Ensure required directories exist
ensure_directories() {
    # Create data directory for database if it doesn't exist
    if [ ! -d "$SCRIPT_DIR/data" ]; then
        log_info "Creating data directory for database..."
        mkdir -p "$SCRIPT_DIR/data"
        chmod 755 "$SCRIPT_DIR/data"
        log_info "✓ Data directory created"
    fi
    
    # Create models directory if it doesn't exist
    if [ ! -d "$SCRIPT_DIR/models" ]; then
        log_info "Creating models directory..."
        mkdir -p "$SCRIPT_DIR/models"
        chmod 755 "$SCRIPT_DIR/models"
        log_info "✓ Models directory created"
    fi
    
    # Create config directory if it doesn't exist
    if [ ! -d "$SCRIPT_DIR/config" ]; then
        mkdir -p "$SCRIPT_DIR/config"
        chmod 755 "$SCRIPT_DIR/config"
    fi
}

# Initialize fresh database file
init_fresh_database() {
    local db_path="$SCRIPT_DIR/data/meeting_minutes.db"
    if [ ! -f "$db_path" ]; then
        log_info "Initializing fresh database..."
        # Create an empty database file with proper permissions
        touch "$db_path"
        chmod 644 "$db_path"
        log_info "✓ Fresh database initialized at: $db_path"
    fi
}

# Ensure directories exist on script start
ensure_directories

# Initialize fresh database if it doesn't exist
init_fresh_database

# Platform detection for macOS support
DETECTED_OS=$(uname -s)
IS_MACOS=false
COMPOSE_PROFILE_ARGS=()
if [[ "$DETECTED_OS" == "Darwin" ]]; then
    IS_MACOS=true
    COMPOSE_PROFILE_ARGS=("--profile" "macos")
    log_info "macOS detected - will use macOS-optimized Docker services"
else
    # Use default profile for Linux/Windows
    COMPOSE_PROFILE_ARGS=("--profile" "default")
fi

# Function to load saved preferences
load_preferences() {
    if [ ! -f "$PREFERENCES_FILE" ]; then
        return 1
    fi
    
    # Source the preferences file safely
    if source "$PREFERENCES_FILE" 2>/dev/null; then
        return 0
    else
        log_warn "Invalid preferences file, will use defaults"
        return 1
    fi
}

# Function to save current preferences
save_preferences() {
    local model="$1"
    local port="$2"
    local app_port="$3"
    local force_mode="$4"
    local language="$5"
    local translate="$6"
    local diarize="$7"
    local db_selection="$8"
    
    cat > "$PREFERENCES_FILE" << EOF
# Docker run preferences - automatically generated
# Last updated: $(date)
SAVED_MODEL="$model"
SAVED_PORT="$port"
SAVED_APP_PORT="$app_port"
SAVED_FORCE_MODE="$force_mode"
SAVED_LANGUAGE="$language"
SAVED_TRANSLATE="$translate"
SAVED_DIARIZE="$diarize"
SAVED_DB_SELECTION="$db_selection"
EOF
    
    log_info "✓ Preferences saved to $PREFERENCES_FILE"
}

# Function to show saved preferences and ask user choice
show_previous_settings() {
    echo -e "${BLUE}=== Previous Settings Found ===${NC}" >&2
    echo -e "${GREEN}Your last configuration:${NC}" >&2
    echo "  Model: ${SAVED_MODEL:-$DEFAULT_MODEL}" >&2
    echo "  Whisper Port: ${SAVED_PORT:-$DEFAULT_PORT}" >&2
    echo "  App Port: ${SAVED_APP_PORT:-$DEFAULT_APP_PORT}" >&2
    echo "  GPU Mode: ${SAVED_FORCE_MODE:-auto}" >&2
    echo "  Language: ${SAVED_LANGUAGE:-auto}" >&2
    echo "  Translation: ${SAVED_TRANSLATE:-false}" >&2
    echo "  Diarization: ${SAVED_DIARIZE:-false}" >&2
    echo "  Database: ${SAVED_DB_SELECTION:-fresh}" >&2
    echo >&2
    
    echo "What would you like to do?" >&2
    echo "  1) Use previous settings" >&2
    echo "  2) Customize settings (interactive setup)" >&2
    echo "  3) Use defaults and skip interactive setup" >&2
    echo >&2
    
    while true; do
        echo -ne "${YELLOW}Choose option [default: 1]: ${NC}" >&2
        read choice
        
        # Default to use previous settings if empty
        if [[ -z "$choice" ]]; then
            choice=1
        fi
        
        case "$choice" in
            1)
                echo "previous"
                return
                ;;
            2)
                echo "customize"
                return
                ;;
            3)
                echo "defaults"
                return
                ;;
            *)
                echo -e "${RED}Invalid choice. Please choose 1, 2, or 3.${NC}" >&2
                ;;
        esac
    done
}

show_help() {
    cat << EOF
Whisper Server and Meeting App Docker Deployment Script

Usage: $0 [COMMAND] [OPTIONS]

COMMANDS:
  start         Start both whisper server and meeting app
  stop          Stop running services
  restart       Restart services
  logs          Show service logs (use --service to specify)
  status        Show service status
  shell         Open shell in running container (use --service to specify)
  clean         Remove containers and images
  build         Build Docker images
  models        Manage whisper models
  gpu-test      Test GPU availability
  setup-db      Setup/migrate database from existing installation
  compose       Pass commands directly to docker_compose

START OPTIONS:
  -m, --model MODEL        Whisper model to use (default: base.en)
  -p, --port PORT         Whisper port to expose (default: 8178)
  --app-port PORT         Meeting app port to expose (default: 5167)
  -g, --gpu               Force GPU mode for whisper
  -c, --cpu               Force CPU mode for whisper
  --language LANG         Language code (default: auto)
  --translate             Enable translation to English
  # --diarize               Enable speaker diarization (feature not available yet)
  -d, --detach            Run in background
  -i, --interactive       Interactive setup with prompts
  --env-file FILE         Load environment from file

LOG/SHELL OPTIONS:
  -s, --service SERVICE   Service to target (whisper|app) (default: both for logs)
  -f, --follow           Follow log output
  -n, --lines N          Number of lines to show (default: 100)

GLOBAL OPTIONS:
  --dry-run               Show commands without executing
  -h, --help              Show this help

Examples:
  # Interactive setup with prompts for model, language, ports, database, etc.
  $0 start --interactive
  
  # Start with default settings (may prompt for missing options)
  $0 start
  
  # Start with large model on port 8081 in background
  $0 start --model large-v3 --port 8081 --detach
  
  # Start with GPU and custom language  
  $0 start --gpu --language es --detach
  
  # Start with translation enabled
  $0 start --model base --translate --language auto --detach
  
  # Build and start interactively
  $0 build cpu && $0 start --interactive
  
  # View whisper logs
  $0 logs --service whisper -f
  
  # View meeting app logs
  $0 logs --service app -f
  
  # Check status of both services
  $0 status
  
  # Database setup (run before first start)
  $0 setup-db                         # Interactive database setup
  $0 setup-db --auto                  # Auto-detect existing database
  
  # Using docker_compose directly
  $0 compose up -d                    # Start both services in background
  $0 compose logs meeting-app         # View app logs
  $0 compose down                     # Stop all services

User Preferences:
  The script automatically saves your configuration choices and offers to reuse them
  on subsequent runs. Preferences are stored in: .docker-preferences
  
  When starting interactively, you'll be offered:
  1) Use previous settings - Reuse your last configuration
  2) Customize settings - Go through interactive setup again  
  3) Use defaults - Skip setup and use default values

Environment Variables:
  WHISPER_MODEL         Default whisper model
  WHISPER_PORT          Default whisper port
  APP_PORT              Default app port
  WHISPER_REGISTRY      Default registry
  WHISPER_GPU           Force GPU mode (true/false)

EOF
}

# Function to detect system capabilities
detect_system() {
    local gpu_available=false
    local gpu_type="none"
    
    # Check for NVIDIA GPU
    if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
        gpu_available=true
        gpu_type="nvidia"
        log_info "NVIDIA GPU detected"
    elif [ -c /dev/nvidiactl ]; then
        gpu_available=true
        gpu_type="nvidia"
        log_info "NVIDIA GPU drivers detected"
    fi
    
    # Check for AMD GPU
    if command -v rocm-smi >/dev/null 2>&1; then
        gpu_available=true
        gpu_type="amd"
        log_info "AMD GPU detected"
    fi
    
    # Check Docker
    if ! command -v docker >/dev/null 2>&1; then
        log_error "Docker is not installed"
        exit 1
    fi
    
    # Check Docker Compose
    local compose_available=false
    if command -v docker-compose >/dev/null 2>&1 || docker compose version >/dev/null 2>&1; then
        compose_available=true
    fi
    
    echo "gpu_available:$gpu_available gpu_type:$gpu_type compose_available:$compose_available"
}

# Function to choose image type
choose_image() {
    local force_mode="$1"
    local registry="${2:-}"
    local system_info
    system_info=$(detect_system)
    
    local gpu_available=$(echo "$system_info" | grep -o 'gpu_available:[^[:space:]]*' | cut -d: -f2)
    local gpu_type=$(echo "$system_info" | grep -o 'gpu_type:[^[:space:]]*' | cut -d: -f2)
    
    local image_tag=""
    local docker_args=()
    
    case "$force_mode" in
        "gpu")
            if [ "$gpu_available" = "true" ]; then
                image_tag="gpu"
                if [ "$gpu_type" = "nvidia" ]; then
                    docker_args+=("--gpus" "all")
                fi
                log_info "Using GPU image (forced)"
            else
                log_warn "GPU forced but no GPU detected, falling back to CPU"
                image_tag="cpu"
            fi
            ;;
        "cpu")
            image_tag="cpu"
            log_info "Using CPU image (forced)"
            ;;
        "auto"|"")
            if [ "$gpu_available" = "true" ]; then
                image_tag="gpu"
                if [ "$gpu_type" = "nvidia" ]; then
                    docker_args+=("--gpus" "all")
                fi
                log_info "Using GPU image (auto-detected)"
            else
                image_tag="cpu"
                log_info "Using CPU image (no GPU detected)"
            fi
            ;;
    esac
    
    local full_image=""
    if [ -n "$registry" ]; then
        full_image="${registry}/${PROJECT_NAME}:${image_tag}"
    else
        full_image="${PROJECT_NAME}:${image_tag}"
    fi
    
    echo "image:$full_image docker_args:${docker_args[*]}"
}

# Function to check if image exists and find best match
check_image() {
    local image="$1"
    
    # First, try exact match
    if docker image inspect "$image" >/dev/null 2>&1; then
        echo "$image"
        return 0
    fi
    
    # If exact match fails, try to find the latest timestamped version
    local image_base="${image%:*}"  # Remove tag part
    local tag="${image##*:}"        # Get tag part
    
    # Look for any images with the same base and tag pattern
    local found_image
    found_image=$(docker images --format "{{.Repository}}:{{.Tag}}" | grep "^${image_base}:${tag}-" | head -1)
    
    if [ -n "$found_image" ]; then
        echo "$found_image"
        return 0
    fi
    
    # No image found
    echo "$image"
    return 1
}

# Function to ensure models directory exists
ensure_models_dir() {
    local models_dir="$SCRIPT_DIR/models"
    
    if [ ! -d "$models_dir" ]; then
        log_info "Creating models directory: $models_dir"
        mkdir -p "$models_dir"
    fi
    
    echo "$models_dir"
}

# Function to show options when user presses Ctrl+C during log viewing
show_log_exit_options() {
    local port="$1"
    local app_port="$2"
    
    echo
    echo
    log_info "=== Log Viewing Options ==="
    
    # Check if services are actually running
    local services_running=false
    if docker ps --format "{{.Names}}" | grep -q "whisper-server\|meetily-backend"; then
        services_running=true
        echo "Services are still running in the background."
    else
        echo "Services appear to have stopped."
    fi
    
    echo
    echo "What would you like to do?"
    if [ "$services_running" = "true" ]; then
        echo "  1) Continue viewing logs"
        echo "  2) Exit log viewing (keep services running)"
        echo "  3) Stop services and exit"
        echo "  4) Restart services"
        echo "  5) Show service status"
    else
        echo "  1) Restart services and continue viewing logs"
        echo "  2) Exit (services are stopped)"
        echo "  3) Show service status"
    fi
    echo
    
    while true; do
        # Check if services are running to determine valid options
        local services_running=false
        if docker ps --format "{{.Names}}" | grep -q "whisper-server\|meetily-backend"; then
            services_running=true
            read -p "$(echo -e "${YELLOW}Choose option (1-5): ${NC}")" choice
        else
            read -p "$(echo -e "${YELLOW}Choose option (1-3): ${NC}")" choice
        fi
        
        if [ "$services_running" = "true" ]; then
            # Services are running - full menu
            case "$choice" in
                1)
                    log_info "Continuing log viewing... (Press Ctrl+C again for options)"
                    echo
                    # Set trap and continue with logs
                    trap 'show_log_exit_options "$port" "$app_port"' INT
                    exec MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" logs -f
                    ;;
                2)
                    log_info "Exiting log viewing. Services remain running."
                    echo
                    log_info "📊 Service URLs:"
                    log_info "  Whisper Server: http://localhost:$port"
                    log_info "  Meeting App: http://localhost:$app_port"
                    echo
                    log_info "📋 Use these commands:"
                    log_info "  View logs:     $0 logs -f"
                    log_info "  Check status:  $0 status"
                    log_info "  Stop services: $0 stop"
                    exit 0
                    ;;
                3)
                    log_info "Stopping services..."
                    MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" down
                    log_info "✓ Services stopped"
                    exit 0
                    ;;
                4)
                    log_info "Restarting services..."
                    MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" restart
                    log_info "✓ Services restarted"
                    log_info "Resuming log viewing... (Press Ctrl+C for options)"
                    echo
                    # Set trap and continue with logs
                    trap 'show_log_exit_options "$port" "$app_port"' INT
                    exec MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" logs -f
                    ;;
                5)
                    echo
                    show_status
                    echo
                    echo "Choose another option:"
                    ;;
                *)
                    echo -e "${RED}Invalid option. Please choose 1-5.${NC}"
                    ;;
            esac
        else
            # Services are stopped - limited menu
            case "$choice" in
                1)
                    log_info "Restarting services..."
                    MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" restart
                    log_info "✓ Services restarted"
                    log_info "Starting log viewing... (Press Ctrl+C for options)"
                    echo
                    # Set trap and continue with logs
                    trap 'show_log_exit_options "$port" "$app_port"' INT
                    exec MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" logs -f
                    ;;
                2)
                    log_info "Exiting. Services remain stopped."
                    exit 0
                    ;;
                3)
                    echo
                    show_status
                    echo
                    echo "Choose another option:"
                    ;;
                *)
                    echo -e "${RED}Invalid option. Please choose 1-3.${NC}"
                    ;;
            esac
        fi
    done
}

# Available whisper models
AVAILABLE_MODELS=(
    "tiny" "tiny.en" "tiny-q5_1"
    "base" "base.en" "base-q5_1"
    "small" "small.en" "small-q5_1"
    "medium" "medium.en" "medium-q5_1"
    "large-v1" "large-v2" "large-v3"
    "large-v1-q5_1" "large-v2-q5_1" "large-v3-q5_1"
    "large-v1-turbo" "large-v2-turbo" "large-v3-turbo"
)

# Function to show interactive model selection
select_model() {
    local default_model="${1:-base.en}"
    
    echo -e "${BLUE}=== Model Selection ===${NC}" >&2
    echo -e "${GREEN}Available Whisper models:${NC}" >&2
    echo >&2
    
    local i=1
    for model in "${AVAILABLE_MODELS[@]}"; do
        if [[ "$model" == "$default_model" ]]; then
            printf "  %2d) %s ${GREEN}(current)${NC}\n" $i "$model" >&2
        else
            printf "  %2d) %s\n" $i "$model" >&2
        fi
        ((i++))
    done
    
    echo >&2
    echo -e "${YELLOW}Model size guide:${NC}" >&2
    echo "  tiny    (~39 MB)  - Fastest, least accurate" >&2
    echo "  base    (~142 MB) - Good balance of speed/accuracy" >&2
    echo "  small   (~244 MB) - Better accuracy" >&2
    echo "  medium  (~769 MB) - High accuracy" >&2
    echo "  large   (~1550 MB)- Best accuracy, slowest" >&2
    echo >&2
    
    while true; do
        echo -ne "${YELLOW}Select model number (1-${#AVAILABLE_MODELS[@]}) or enter model name [default: $default_model]: ${NC}" >&2
        read choice
        
        # Default to saved preference if empty
        if [[ -z "$choice" ]]; then
            echo "$default_model"
            return
        fi
        
        # Check if it's a number
        if [[ "$choice" =~ ^[0-9]+$ ]]; then
            if [[ $choice -ge 1 && $choice -le ${#AVAILABLE_MODELS[@]} ]]; then
                echo "${AVAILABLE_MODELS[$((choice-1))]}"
                return
            else
                echo -e "${RED}Invalid selection. Please choose 1-${#AVAILABLE_MODELS[@]}${NC}" >&2
                continue
            fi
        else
            # Check if it's a valid model name
            for model in "${AVAILABLE_MODELS[@]}"; do
                if [[ "$choice" == "$model" ]]; then
                    echo "$choice"
                    return
                fi
            done
            echo -e "${RED}Invalid model name. Please choose from available models.${NC}" >&2
        fi
    done
}

# Function to show interactive language selection
select_language() {
    local default_language="${1:-auto}"
    
    echo -e "${BLUE}=== Language Selection ===${NC}" >&2
    echo -e "${GREEN}Common languages:${NC}" >&2
    
    # Helper function to show current marker
    show_option() {
        local num="$1"
        local code="$2"
        local name="$3"
        if [[ "$code" == "$default_language" ]]; then
            printf "%3s) %s ${GREEN}(current)${NC}\n" "$num" "$name" >&2
        else
            printf "%3s) %s\n" "$num" "$name" >&2
        fi
    }
    
    show_option "1" "auto" "auto (automatic detection)"
    show_option "2" "en" "en (English)"
    show_option "3" "es" "es (Spanish)"
    show_option "4" "fr" "fr (French)"
    show_option "5" "de" "de (German)"
    show_option "6" "it" "it (Italian)"
    show_option "7" "pt" "pt (Portuguese)"
    show_option "8" "ru" "ru (Russian)"
    show_option "9" "ja" "ja (Japanese)"
    show_option "10" "zh" "zh (Chinese)"
    echo " 11) Other (enter language code)" >&2
    echo >&2
    
    while true; do
        echo -ne "${YELLOW}Select language [default: $default_language]: ${NC}" >&2
        read choice
        
        case "$choice" in
            ""|"1") echo "auto"; return ;;
            "2") echo "en"; return ;;
            "3") echo "es"; return ;;
            "4") echo "fr"; return ;;
            "5") echo "de"; return ;;
            "6") echo "it"; return ;;
            "7") echo "pt"; return ;;
            "8") echo "ru"; return ;;
            "9") echo "ja"; return ;;
            "10") echo "zh"; return ;;
            "11")
                echo -ne "${YELLOW}Enter language code (e.g., ko, ar, hi): ${NC}" >&2
                read lang_code
                if [[ -n "$lang_code" ]]; then
                    echo "$lang_code"
                    return
                else
                    echo "$default_language"
                    return
                fi
                ;;
            "")
                echo "$default_language"
                return
                ;;
            *)
                # Check if it's a direct language code
                if [[ "$choice" =~ ^[a-z]{2}$ ]]; then
                    echo "$choice"
                    return
                else
                    echo -e "${RED}Invalid selection. Please choose 1-11 or enter a valid language code.${NC}" >&2
                fi
                ;;
        esac
    done
}

# Function to check if port is available
check_port_available() {
    local port="$1"
    if lsof -i ":$port" | grep -q LISTEN 2>/dev/null; then
        return 1  # Port is in use
    else
        return 0  # Port is available
    fi
}

# Function to select whisper server port
select_whisper_port() {
    local default_port="${1:-8178}"
    
    echo -e "${BLUE}=== Whisper Server Port Selection ===${NC}" >&2
    echo -e "${GREEN}Choose Whisper server port:${NC}" >&2
    echo "  Current: $default_port" >&2
    echo "  Common alternatives: 8081, 8082, 8178, 9080" >&2
    echo >&2
    
    while true; do
        echo -ne "${YELLOW}Enter Whisper server port [default: $default_port]: ${NC}" >&2
        read port_choice
        
        # Default to saved preference if empty
        if [[ -z "$port_choice" ]]; then
            echo "$default_port"
            return
        fi
        
        # Validate port number
        if [[ "$port_choice" =~ ^[0-9]+$ ]] && [[ $port_choice -ge 1024 && $port_choice -le 65535 ]]; then
            if check_port_available "$port_choice"; then
                echo "$port_choice"
                return
            else
                echo -e "${RED}Port $port_choice is already in use.${NC}" >&2
                echo -ne "${YELLOW}Kill the process using this port? (y/N): ${NC}" >&2
                read kill_choice
                if [[ "$kill_choice" =~ ^[Yy] ]]; then
                    if lsof -ti ":$port_choice" | xargs kill -9 2>/dev/null; then
                        echo -e "${GREEN}Port $port_choice is now available.${NC}" >&2
                        echo "$port_choice"
                        return
                    else
                        echo -e "${RED}Failed to free port $port_choice.${NC}" >&2
                    fi
                fi
            fi
        else
            echo -e "${RED}Invalid port. Please enter a number between 1024-65535.${NC}" >&2
        fi
    done
}

# Function to select meeting app port
select_app_port() {
    local default_port="${1:-5167}"
    
    echo -e "${BLUE}=== Meeting App Port Selection ===${NC}" >&2
    echo -e "${GREEN}Choose Meeting app port:${NC}" >&2
    echo "  Current: $default_port" >&2
    echo "  Common alternatives: 5168, 5169, 3000, 8000" >&2
    echo >&2
    
    while true; do
        echo -ne "${YELLOW}Enter Meeting app port [default: $default_port]: ${NC}" >&2
        read port_choice
        
        # Default to saved preference if empty
        if [[ -z "$port_choice" ]]; then
            echo "$default_port"
            return
        fi
        
        # Validate port number
        if [[ "$port_choice" =~ ^[0-9]+$ ]] && [[ $port_choice -ge 1024 && $port_choice -le 65535 ]]; then
            if check_port_available "$port_choice"; then
                echo "$port_choice"
                return
            else
                echo -e "${RED}Port $port_choice is already in use.${NC}" >&2
                echo -ne "${YELLOW}Kill the process using this port? (y/N): ${NC}" >&2
                read kill_choice
                if [[ "$kill_choice" =~ ^[Yy] ]]; then
                    if lsof -ti ":$port_choice" | xargs kill -9 2>/dev/null; then
                        echo -e "${GREEN}Port $port_choice is now available.${NC}" >&2
                        echo "$port_choice"
                        return
                    else
                        echo -e "${RED}Failed to free port $port_choice.${NC}" >&2
                    fi
                fi
            fi
        else
            echo -e "${RED}Invalid port. Please enter a number between 1024-65535.${NC}" >&2
        fi
    done
}

# Function to check if database exists and is valid
check_database() {
    local db_path="$1"
    
    if [ ! -f "$db_path" ]; then
        return 1
    fi
    
    # Check if it's a valid SQLite database
    if ! sqlite3 "$db_path" "SELECT name FROM sqlite_master WHERE type='table' LIMIT 1;" >/dev/null 2>&1; then
        return 1
    fi
    
    return 0
}

# Function to get database info
get_database_info() {
    local db_path="$1"
    
    echo "  Path: $db_path" >&2
    echo "  Size: $(du -h "$db_path" | cut -f1)" >&2
    echo "  Modified: $(stat -f "%Sm" "$db_path" 2>/dev/null || stat -c "%y" "$db_path" 2>/dev/null || echo "Unknown")" >&2
    
    # Try to get table counts
    local meetings_count=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM meetings;" 2>/dev/null || echo "0")
    local transcripts_count=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM transcripts;" 2>/dev/null || echo "0")
    
    echo "  Meetings: $meetings_count" >&2
    echo "  Transcripts: $transcripts_count" >&2
}

# Function to find existing databases
find_existing_databases() {
    local found_dbs=()
    local default_db_path="/opt/homebrew/Cellar/meetily-backend/0.0.4/backend/meeting_minutes.db"
    
    # Check default location
    if check_database "$default_db_path"; then
        found_dbs+=("$default_db_path")
    fi
    
    # Check other common locations
    local common_paths=(
        "/opt/homebrew/Cellar/meetily-backend/*/backend/meeting_minutes.db"
        "$HOME/.meetily/meeting_minutes.db"
        "$HOME/Documents/meetily/meeting_minutes.db"
        "$HOME/Desktop/meeting_minutes.db"
        "./meeting_minutes.db"
        "$SCRIPT_DIR/data/meeting_minutes.db"
    )
    
    for pattern in "${common_paths[@]}"; do
        for path in $pattern; do
            if [[ "$path" != "$default_db_path" ]] && check_database "$path"; then
                found_dbs+=("$path")
            fi
        done
    done
    
    printf '%s\n' "${found_dbs[@]}"
}

# Function to select database setup option
select_database_setup() {
    echo -e "${BLUE}=== Database Setup Selection ===${NC}" >&2
    echo -e "${GREEN}Choose database setup option:${NC}" >&2
    echo >&2
    
    # Search for existing databases
    local found_dbs=($(find_existing_databases))
    
    if [ ${#found_dbs[@]} -eq 0 ]; then
        echo "  1) Fresh installation (create new database)" >&2
        echo "  2) I have an existing database at a custom location" >&2
        echo >&2
        
        while true; do
            echo -ne "${YELLOW}Select database option [default: 1]: ${NC}" >&2
            read db_choice
            
            # Default to fresh if empty
            if [[ -z "$db_choice" ]]; then
                echo "fresh"
                return
            fi
            
            case "$db_choice" in
                1)
                    echo "fresh"
                    return
                    ;;
                2)
                    echo -ne "${YELLOW}Enter the full path to your existing database: ${NC}" >&2
                    read custom_path
                    if check_database "$custom_path"; then
                        echo -e "${GREEN}Database found:${NC}" >&2
                        get_database_info "$custom_path"
                        echo >&2
                        echo -ne "${YELLOW}Use this database? (Y/n): ${NC}" >&2
                        read confirm
                        if [[ ! "$confirm" =~ ^[Nn]$ ]]; then
                            echo "$custom_path"
                            return
                        fi
                    else
                        echo -e "${RED}Invalid database file: $custom_path${NC}" >&2
                    fi
                    ;;
                *)
                    echo -e "${RED}Invalid choice. Please choose 1 or 2.${NC}" >&2
                    ;;
            esac
        done
    else
        echo -e "${GREEN}Found ${#found_dbs[@]} existing database(s):${NC}" >&2
        echo >&2
        
        local i=1
        for db in "${found_dbs[@]}"; do
            echo "  $i) $db" >&2
            ((i++))
        done
        echo "  $i) Use custom path" >&2
        ((i++))
        echo "  $i) Fresh installation" >&2
        echo >&2
        
        while true; do
            echo -ne "${YELLOW}Select database option [default: 1]: ${NC}" >&2
            read db_choice
            
            # Default to first found database if empty
            if [[ -z "$db_choice" ]]; then
                db_choice=1
            fi
            
            if [[ "$db_choice" =~ ^[0-9]+$ ]] && [[ $db_choice -ge 1 && $db_choice -le ${#found_dbs[@]} ]]; then
                local selected_db="${found_dbs[$((db_choice-1))]}"
                echo -e "${GREEN}Selected database:${NC}" >&2
                get_database_info "$selected_db"
                echo >&2
                echo -ne "${YELLOW}Use this database? (Y/n): ${NC}" >&2
                read confirm
                if [[ ! "$confirm" =~ ^[Nn]$ ]]; then
                    echo "$selected_db"
                    return
                fi
            elif [[ $db_choice -eq $((${#found_dbs[@]}+1)) ]]; then
                echo -ne "${YELLOW}Enter the full path to your existing database: ${NC}" >&2
                read custom_path
                if check_database "$custom_path"; then
                    echo -e "${GREEN}Database found:${NC}" >&2
                    get_database_info "$custom_path"
                    echo >&2
                    echo -ne "${YELLOW}Use this database? (Y/n): ${NC}" >&2
                    read confirm
                    if [[ "$confirm" =~ ^[Yy]$ ]]; then
                        echo "$custom_path"
                        return
                    fi
                else
                    echo -e "${RED}Invalid database file: $custom_path${NC}" >&2
                fi
            elif [[ $db_choice -eq $((${#found_dbs[@]}+2)) ]]; then
                echo "fresh"
                return
            else
                echo -e "${RED}Invalid choice. Please choose 1-$((${#found_dbs[@]}+2)).${NC}" >&2
            fi
        done
    fi
}

# Function to check if model exists and download if needed
ensure_model_available() {
    local model="$1"
    local models_dir=$(ensure_models_dir)
    local model_file="$models_dir/ggml-${model}.bin"
    
    if [[ -f "$model_file" ]]; then
        local file_size=$(du -h "$model_file" | cut -f1)
        log_info "✅ Model already available: $model ($file_size)"
        return 0
    fi
    
    log_warn "⚠️  Model not found locally: $model"
    
    # Show estimated download size
    case "$model" in
        tiny*) log_info "📦 Estimated download size: ~39 MB (fastest, least accurate)" ;;
        base*) log_info "📦 Estimated download size: ~142 MB (good balance)" ;;
        small*) log_info "📦 Estimated download size: ~244 MB (better accuracy)" ;;
        medium*) log_info "📦 Estimated download size: ~769 MB (high accuracy)" ;;
        large*) log_info "📦 Estimated download size: ~1550 MB (best accuracy)" ;;
    esac
    
    echo
    log_info "💡 Model download options:"
    log_info "   1. Download now (recommended for faster startup)"
    log_info "   2. Auto-download in container (slower startup, but automated)"
    echo
    
    # Ask user preference if running interactively
    if [[ -t 0 && -t 1 ]]; then
        read -p "$(echo -e "${YELLOW}Download model now? (Y/n): ${NC}")" download_choice
        
        if [[ ! "$download_choice" =~ ^[Nn] ]]; then
            log_info "🔄 Downloading model now..."
            if manage_models download "$model"; then
                log_info "✅ Model ready for immediate use!"
                return 0
            else
                log_warn "⚠️  Pre-download failed, will auto-download in container"
            fi
        else
            log_info "📌 Model will be downloaded automatically in the container"
        fi
    else
        log_info "📌 Model will be downloaded automatically in the container"
    fi
    
    return 0
}

# Function to start both services using docker_compose
start_server() {
    local model="$DEFAULT_MODEL"
    local port="$DEFAULT_PORT"
    local app_port="$DEFAULT_APP_PORT"
    local force_mode="auto"
    local detach=false
    local env_file=""
    local extra_args=()
    local compose_env=()
    local language=""
    local translate="false"
    # local diarize="false"  # Feature not available yet
    local interactive=false
    
    # Parse options
    while [[ $# -gt 0 ]]; do
        case $1 in
            -m|--model)
                model="$2"
                shift 2
                ;;
            -p|--port)
                port="$2"
                shift 2
                ;;
            --app-port)
                app_port="$2"
                shift 2
                ;;
            -g|--gpu)
                force_mode="gpu"
                shift
                ;;
            -c|--cpu)
                force_mode="cpu"
                shift
                ;;
            --language)
                language="$2"
                shift 2
                ;;
            --translate)
                translate="true"
                shift
                ;;
            # --diarize)  # Feature not available yet
            #     diarize="true"
            #     shift
            #     ;;
            -d|--detach)
                detach=true
                shift
                ;;
            -i|--interactive)
                interactive=true
                shift
                ;;
            --env-file)
                env_file="$2"
                shift 2
                ;;
            *)
                extra_args+=("$1")
                shift
                ;;
        esac
    done
    
    # Check if we should run interactive mode and handle preferences
    local run_interactive=false
    local setup_mode="interactive"
    local has_saved_preferences=false
    
    # Try to load saved preferences
    if load_preferences; then
        has_saved_preferences=true
    fi
    
    if [[ "$interactive" == "true" ]]; then
        run_interactive=true
        if [[ "$has_saved_preferences" == "true" ]]; then
            setup_mode=$(show_previous_settings)
        else
            setup_mode="customize"
        fi
    elif [[ "$model" == "$DEFAULT_MODEL" && -z "$language" && -t 0 && -t 1 ]]; then
        # Only auto-prompt if running interactively (not in scripts/pipes)
        run_interactive=true
        if [[ "$has_saved_preferences" == "true" ]]; then
            setup_mode=$(show_previous_settings)
        else
            setup_mode="customize"
        fi
    fi
    
    # Interactive mode - prompt for settings
    if [[ "$run_interactive" == "true" ]]; then
        local db_selection="fresh"
        local db_setup_needed=""
        
        case "$setup_mode" in
            "previous")
                # Use saved preferences
                echo -e "${GREEN}=== Using Previous Settings ===${NC}"
                model="${SAVED_MODEL:-$model}"
                port="${SAVED_PORT:-$port}"
                app_port="${SAVED_APP_PORT:-$app_port}"
                force_mode="${SAVED_FORCE_MODE:-$force_mode}"
                language="${SAVED_LANGUAGE:-$language}"
                translate="${SAVED_TRANSLATE:-$translate}"
                # diarize="${SAVED_DIARIZE:-$diarize}"  # Feature not available yet
                db_selection="${SAVED_DB_SELECTION:-fresh}"
                
                log_info "✓ Loaded previous configuration"
                echo
                ;;
            "defaults")
                # Use defaults, skip interactive setup
                echo -e "${GREEN}=== Using Default Settings ===${NC}"
                log_info "✓ Using default configuration"
                echo
                ;;
            "customize")
                # Full interactive setup with saved preferences as defaults
                echo -e "${GREEN}=== Interactive Setup ===${NC}"
                echo
                
                # Model selection - always show, using saved preference as default
                echo -e "${BLUE}🎯 Model Selection${NC}"
                local current_model="${SAVED_MODEL:-$model}"
                model=$(select_model "$current_model")
                echo -e "${GREEN}Selected model: $model${NC}"
                echo
                
                # Language selection - always show, using saved preference as default
                echo -e "${BLUE}🌐 Language Selection${NC}"
                local current_language="${SAVED_LANGUAGE:-$language}"
                language=$(select_language "$current_language")
                echo -e "${GREEN}Selected language: $language${NC}"
                echo
                
                # Port selection - always show, using saved preference as default
                echo -e "${BLUE}🔌 Whisper Server Port Selection${NC}"
                local current_port="${SAVED_PORT:-$port}"
                port=$(select_whisper_port "$current_port")
                echo -e "${GREEN}Selected Whisper port: $port${NC}"
                echo
                
                echo -e "${BLUE}🔌 Meeting App Port Selection${NC}"
                local current_app_port="${SAVED_APP_PORT:-$app_port}"
                app_port=$(select_app_port "$current_app_port")
                echo -e "${GREEN}Selected Meeting app port: $app_port${NC}"
                echo
                
                # Database setup selection
                echo -e "${BLUE}🗄️ Database Setup Selection${NC}"
                # Check if sqlite3 is available for database operations
                if command -v sqlite3 >/dev/null 2>&1; then
                    db_selection=$(select_database_setup)
                    if [[ "$db_selection" == "fresh" ]]; then
                        echo -e "${GREEN}Selected: Fresh database installation${NC}"
                    else
                        echo -e "${GREEN}Selected database: $db_selection${NC}"
                        # Set up database copy for the selected database
                        db_setup_needed="$db_selection"
                    fi
                else
                    echo -e "${YELLOW}sqlite3 not found, will use fresh database installation${NC}"
                    db_selection="fresh"
                fi
                echo
                
                # GPU mode selection
                if [[ "$force_mode" == "auto" ]]; then
                    local system_info
                    system_info=$(detect_system)
                    local gpu_available=$(echo "$system_info" | grep -o 'gpu_available:[^[:space:]]*' | cut -d: -f2)
                    
                    if [[ "$gpu_available" == "true" ]]; then
                        echo
                        local saved_gpu_mode="${SAVED_FORCE_MODE:-auto}"
                        local gpu_default="Y"
                        if [[ "$saved_gpu_mode" == "cpu" ]]; then
                            gpu_default="n"
                        fi
                        read -p "$(echo -e "${YELLOW}GPU detected. Use GPU acceleration? (Y/n) [current: $saved_gpu_mode]: ${NC}")" gpu_choice
                        gpu_choice="${gpu_choice:-$gpu_default}"
                        if [[ "$gpu_choice" =~ ^[Nn] ]]; then
                            force_mode="cpu"
                        else
                            force_mode="gpu"
                        fi
                    else
                        log_info "No GPU detected, using CPU mode"
                        force_mode="cpu"
                    fi
                fi
                
                # Advanced options
                echo
                local saved_translate="${SAVED_TRANSLATE:-false}"
                local translate_default="N"
                if [[ "$saved_translate" == "true" ]]; then
                    translate_default="y"
                fi
                read -p "$(echo -e "${YELLOW}Enable translation to English? (y/N) [current: $saved_translate]: ${NC}")" translate_choice
                translate_choice="${translate_choice:-$translate_default}"
                if [[ "$translate_choice" =~ ^[Yy] ]]; then
                    translate="true"
                fi
                
                # local saved_diarize="${SAVED_DIARIZE:-false}"
                # local diarize_default="N"
                # if [[ "$saved_diarize" == "true" ]]; then
                #     diarize_default="y"
                # fi
                # read -p "$(echo -e "${YELLOW}Enable speaker diarization? (y/N) [current: $saved_diarize]: ${NC}")" diarize_choice
                # diarize_choice="${diarize_choice:-$diarize_default}"
                # if [[ "$diarize_choice" =~ ^[Yy] ]]; then
                #     diarize="true"
                # fi
                
                # Save the new preferences
                # save_preferences "$model" "$port" "$app_port" "$force_mode" "$language" "$translate" "$diarize" "$db_selection"
                save_preferences "$model" "$port" "$app_port" "$force_mode" "$language" "$translate" "false" "$db_selection"
                echo
                ;;
        esac
        
        # Handle database setup for all modes
        if [[ "$db_selection" != "fresh" && -n "$db_selection" ]]; then
            db_setup_needed="$db_selection"
        fi
        
        # If sqlite3 is not available and we're not in customize mode, ensure db_selection is set to fresh
        # and update preferences if we loaded previous settings
        if ! command -v sqlite3 >/dev/null 2>&1 && [[ "$setup_mode" != "customize" ]]; then
            if [[ "$db_selection" != "fresh" ]]; then
                log_warn "sqlite3 not found, switching to fresh database installation"
                db_selection="fresh"
                # Update preferences with fresh db_selection
                if [[ "$setup_mode" == "previous" ]]; then
                    save_preferences "$model" "$port" "$app_port" "$force_mode" "$language" "$translate" "$diarize" "$db_selection"
                fi
            fi
        fi
    fi
    
    # Use environment variables if set
    model="${WHISPER_MODEL:-$model}"
    port="${WHISPER_PORT:-$port}"
    app_port="${APP_PORT:-$app_port}"
    
    # Handle database setup if needed
    if [[ -n "${db_setup_needed:-}" ]]; then
        log_info "Setting up database from selected source..."
        local docker_db_dir="$SCRIPT_DIR/data"
        local docker_db_path="$docker_db_dir/meeting_minutes.db"
        
        # Create data directory
        mkdir -p "$docker_db_dir"
        
        # Copy the selected database
        if cp "$db_setup_needed" "$docker_db_path"; then
            chmod 644 "$docker_db_path"
            log_info "✓ Database setup complete: $docker_db_path"
        else
            log_error "Failed to setup database from $db_setup_needed"
            log_info "Continuing with fresh database setup..."
        fi
    elif [[ "${db_selection:-}" == "fresh" && "$run_interactive" == "true" ]]; then
        log_info "Setting up fresh database..."
        init_fresh_database
        local docker_db_dir="$SCRIPT_DIR/data"
        local docker_db_path="$docker_db_dir/meeting_minutes.db"
        
        # Create data directory
        mkdir -p "$docker_db_dir"
        
        # Ensure database file exists
        if [ ! -f "$docker_db_path" ]; then
            touch "$docker_db_path"
            chmod 644 "$docker_db_path"
        fi
        
        log_info "✓ Fresh database setup complete at: $docker_db_path"
    fi
    
    # Check model availability and show download info
    ensure_model_available "$model"
    
    # Determine dockerfile based on force_mode
    local dockerfile=""
    case "$force_mode" in
        "gpu")
            dockerfile="Dockerfile.server-gpu"
            log_info "Using GPU mode"
            ;;
        "cpu")
            dockerfile="Dockerfile.server-cpu"
            log_info "Using CPU mode"
            ;;
        "auto"|"")
            # Auto-detect GPU
            local system_info
            system_info=$(detect_system)
            local gpu_available=$(echo "$system_info" | grep -o 'gpu_available:[^[:space:]]*' | cut -d: -f2)
            
            if [ "$gpu_available" = "true" ]; then
                dockerfile="Dockerfile.server-gpu"
                log_info "GPU detected, using GPU mode"
            else
                dockerfile="Dockerfile.server-cpu"
                log_info "No GPU detected, using CPU mode"
            fi
            ;;
    esac
    
    # Convert model name to proper path format for whisper.cpp
    local whisper_model_path=""
    if [[ "$model" =~ ^models/ ]]; then
        # Already in path format
        whisper_model_path="$model"
    else
        # Convert model name to path format
        whisper_model_path="models/ggml-${model}.bin"
    fi
    
    # Build environment variables for docker_compose
    compose_env+=("DOCKERFILE=$dockerfile")
    compose_env+=("WHISPER_MODEL=$whisper_model_path")
    compose_env+=("WHISPER_PORT=$port")
    compose_env+=("APP_PORT=$app_port")
    compose_env+=("MODEL_NAME=$model")  # For model-downloader compatibility
    
    if [ -n "$language" ]; then
        compose_env+=("WHISPER_LANGUAGE=$language")
    fi
    if [ "$translate" = "true" ]; then
        compose_env+=("WHISPER_TRANSLATE=true")
    fi
    # if [ "$diarize" = "true" ]; then  # Feature not available yet
    #     compose_env+=("WHISPER_DIARIZE=true")
    # fi
    
    # Check if images exist, build if needed
    local build_type=""
    if [[ "$dockerfile" == *"gpu"* ]]; then
        build_type="gpu"
    else
        build_type="cpu"
    fi
    
    # Check if both images exist
    local whisper_image_exists=false
    local app_image_exists=false
    
    if docker images --format "{{.Repository}}:{{.Tag}}" | grep -q "whisper-server:$build_type"; then
        whisper_image_exists=true
    fi
    
    if docker images --format "{{.Repository}}:{{.Tag}}" | grep -q "meetily-backend:"; then
        app_image_exists=true
    fi
    
    # Build images if they don't exist
    if [ "$whisper_image_exists" = "false" ] || [ "$app_image_exists" = "false" ]; then
        log_info "Some images missing, building..."
        if [ "$DRY_RUN" != "true" ]; then
            "$SCRIPT_DIR/build-docker.sh" "$build_type"
        fi
    fi
    
    # Prepare docker_compose command
    local compose_cmd=()
    
    # Add environment variables
    for env_var in "${compose_env[@]}"; do
        compose_cmd+=("$env_var")
    done
    
    # Docker compose will be called with env command
    
    # Add env-file if specified
    if [ -n "$env_file" ]; then
        compose_cmd+=("--env-file" "$env_file")
    fi
    
    compose_cmd+=("up")
    
    # Add detach flag
    if [ "$detach" = "true" ]; then
        compose_cmd+=("-d")
    fi
    
    log_info "Starting Whisper Server + Meeting App..."
    log_info "Whisper Model: $whisper_model_path"
    log_info "Whisper Port: $port"
    log_info "Meeting App Port: $app_port"
    log_info "Docker mode: $dockerfile"
    
    if [ -n "$language" ]; then
        log_info "Language: $language"
    fi
    if [ "$translate" = "true" ]; then
        log_info "Translation: enabled"
    fi
    # if [ "$diarize" = "true" ]; then  # Feature not available yet
    #     log_info "Diarization: enabled"
    # fi
    
    if [ "$DRY_RUN" = "true" ]; then
        log_info "DRY RUN - Command would be:"
        echo "${compose_cmd[@]}"
        return 0
    fi
    
    # Execute docker_compose
    if [ "$detach" = "true" ]; then
        log_info "Starting services in background..."
        # Export environment variables and run docker_compose
        (
            export "${compose_env[@]}"
            docker_compose "${COMPOSE_PROFILE_ARGS[@]}" up -d ${env_file:+--env-file "$env_file"}
        )
        if [ $? -eq 0 ]; then
            log_info "✓ Services started in background"
            echo
            log_info "📊 Service URLs:"
            log_info "  Whisper Server: http://localhost:$port"
            log_info "  Meeting App: http://localhost:$app_port"
            echo
            log_info "📋 Useful commands:"
            log_info "  View logs:     $0 logs -f"
            log_info "  Check status:  $0 status"
            log_info "  Stop services: $0 stop"
            echo
            
            # Check for model availability and wait for services to initialize
            log_info "🔍 Checking model availability and service initialization..."
            
            # Function to check if model is available in container
            check_model_available() {
                local model_name="$1"
                # Check if model file exists and is not empty in the whisper container
                if docker exec whisper-server test -s "/app/models/ggml-${model_name}.bin" 2>/dev/null; then
                    return 0
                else
                    return 1
                fi
            }
            
            # Wait for model to be available
            local max_wait=300  # 5 minutes max wait for model download
            local wait_count=0
            local model_ready=false
            local model_name="${model##*/}"  # Extract filename from path
            model_name="${model_name#ggml-}"  # Remove ggml- prefix
            model_name="${model_name%.bin}"   # Remove .bin suffix
            
            log_info "⏳ Waiting for model '$model_name' to be ready..."
            
            while [ $wait_count -lt $max_wait ]; do
                if check_model_available "$model_name"; then
                    log_info "✅ Model is ready: $model_name"
                    model_ready=true
                    break
                fi
                
                # Show progress every 30 seconds
                if [ $((wait_count % 30)) -eq 0 ] && [ $wait_count -gt 0 ]; then
                    log_info "⏳ Still downloading model '$model_name'... (${wait_count}s elapsed)"
                fi
                
                sleep 5
                ((wait_count += 5))
            done
            
            if ! $model_ready; then
                log_warn "⚠️  Model download taking longer than expected. Check logs: $0 logs --service whisper -f"
            fi
            
            # Now wait for services to respond
            log_info "⏳ Waiting for services to respond..."
            local service_wait=60  # 1 minute for services to respond after model is ready
            local service_count=0
            local whisper_ready=false
            local app_ready=false
            
            while [ $service_count -lt $service_wait ]; do
                # Check if whisper server is responding
                if ! $whisper_ready && curl -s --connect-timeout 3 "http://localhost:$port/" >/dev/null 2>&1; then
                    log_info "✅ Whisper Server is responding"
                    whisper_ready=true
                fi
                
                # Check if meeting app is responding  
                if ! $app_ready && curl -s --connect-timeout 3 "http://localhost:$app_port/get-meetings" >/dev/null 2>&1; then
                    log_info "✅ Meeting App is responding" 
                    app_ready=true
                fi
                
                # Both services ready
                if $whisper_ready && $app_ready; then
                    log_info "🎉 All services are ready!"
                    break
                fi
                
                sleep 3
                ((service_count += 3))
            done
            
            # Final status check
            if ! $whisper_ready && ! $app_ready; then
                log_warn "⚠️  Services may still be starting up. Check logs: $0 logs -f"
            elif ! $whisper_ready; then
                log_warn "⚠️  Whisper Server not responding. Check logs: $0 logs --service whisper -f"
            elif ! $app_ready; then
                log_warn "⚠️  Meeting App not responding. Check logs: $0 logs --service app -f"
            fi
        else
            log_error "✗ Failed to start services"
            return 1
        fi
    else
        log_info "Starting services with live logs..."
        log_info "Press Ctrl+C to view options for stopping/continuing"
        echo
        
        # Start services in detached mode first
        # Export environment variables and run docker_compose
        (
            export "${compose_env[@]}"
            docker_compose "${COMPOSE_PROFILE_ARGS[@]}" up -d ${env_file:+--env-file "$env_file"}
        )
        if [ $? -eq 0 ]; then
            log_info "✓ Services started in background"
            
            # Now follow logs with trap handling
            # Set up trap for Ctrl+C to show options
            trap 'show_log_exit_options "$port" "$app_port"' INT
            
            # Follow logs - this way docker_compose doesn't handle the interrupt
            MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" logs -f
            local exit_code=$?
            
            # Reset trap to default
            trap - INT
            
            if [ $exit_code -eq 0 ]; then
                log_info "✓ Log viewing stopped normally"
            else
                log_info "✓ Log viewing interrupted"
            fi
        else
            log_error "✗ Failed to start services"
            return 1
        fi
    fi
}

# Function to stop services
stop_server() {
    log_info "Stopping services..."
    if [ "$DRY_RUN" = "true" ]; then
        log_info "DRY RUN - Would run: docker_compose down"
        return 0
    fi
    
    if MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" down; then
        log_info "✓ Services stopped"
    else
        log_error "✗ Failed to stop services"
        return 1
    fi
}

# Function to show logs
show_logs() {
    local follow=false
    local lines=100
    local service=""
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            -f|--follow)
                follow=true
                shift
                ;;
            -n|--lines)
                lines="$2"
                shift 2
                ;;
            --service)
                service="$2"
                shift 2
                ;;
            -s)
                service="$2"
                shift 2
                ;;
            *)
                shift
                ;;
        esac
    done
    
    local log_cmd=("docker_compose" "${COMPOSE_PROFILE_ARGS[@]}" "logs" "--tail=$lines")
    
    if [ "$follow" = "true" ]; then
        log_cmd+=("-f")
    fi
    
    # Add service if specified
    case "$service" in
        "whisper")
            log_cmd+=("whisper-server")
            ;;
        "app"|"backend")
            log_cmd+=("meetily-backend")
            ;;
        "")
            # Show logs from both services
            ;;
        *)
            log_cmd+=("$service")
            ;;
    esac
    
    if [ "$DRY_RUN" = "true" ]; then
        log_info "DRY RUN - Would run: ${log_cmd[*]}"
        return 0
    fi
    
    # Set MODEL_NAME to suppress warnings
    MODEL_NAME="$DEFAULT_MODEL" "${log_cmd[@]}"
}

# Function to show status
show_status() {
    log_info "=== Services Status ==="
    
    if [ "$DRY_RUN" = "true" ]; then
        log_info "DRY RUN - Would run: docker_compose ps"
        return 0
    fi
    
    # Show docker_compose status
    MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" ps
    
    # Check individual service health
    local whisper_running=false
    local app_running=false
    
    if docker ps --format "{{.Names}}" | grep -q "whisper-server"; then
        whisper_running=true
        local whisper_port=$(docker port whisper-server 8178/tcp 2>/dev/null | cut -d: -f2)
        if [ -n "$whisper_port" ]; then
            log_info "Whisper Server: http://localhost:$whisper_port"
            # Test connectivity
            if curl -s --connect-timeout 2 "http://localhost:$whisper_port/" >/dev/null 2>&1; then
                log_info "✓ Whisper Server is responding"
            else
                log_warn "✗ Whisper Server is not responding"
            fi
        fi
    fi
    
    if docker ps --format "{{.Names}}" | grep -q "meetily-backend"; then
        app_running=true
        local app_port=$(docker port meetily-backend 5167/tcp 2>/dev/null | cut -d: -f2)
        if [ -n "$app_port" ]; then
            log_info "Meeting App: http://localhost:$app_port"
            # Test connectivity
            if curl -s --connect-timeout 2 "http://localhost:$app_port/get-meetings" >/dev/null 2>&1; then
                log_info "✓ Meeting App is responding"
            else
                log_warn "✗ Meeting App is not responding"
            fi
        fi
    fi
    
    if [ "$whisper_running" = "false" ] && [ "$app_running" = "false" ]; then
        log_warn "✗ No services are running"
    fi
}

# Function to open shell
open_shell() {
    local service="whisper"
    
    # Parse service option
    while [[ $# -gt 0 ]]; do
        case $1 in
            --service)
                service="$2"
                shift 2
                ;;
            -s)
                service="$2"
                shift 2
                ;;
            *)
                shift
                ;;
        esac
    done
    
    local container_name=""
    case "$service" in
        "whisper")
            container_name="whisper-server"
            ;;
        "app"|"backend")
            container_name="meetily-backend"
            ;;
        *)
            container_name="$service"
            ;;
    esac
    
    if docker ps -q -f name="$container_name" | grep -q .; then
        log_info "Opening shell in $container_name..."
        docker exec -it "$container_name" bash
    else
        log_error "Container $container_name is not running"
        return 1
    fi
}

# Function to clean up
clean_up() {
    local remove_images=false
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            --images)
                remove_images=true
                shift
                ;;
            *)
                shift
                ;;
        esac
    done
    
    log_info "Cleaning up services..."
    
    if [ "$DRY_RUN" = "true" ]; then
        log_info "DRY RUN - Would run:"
        log_info "  docker_compose down"
        if [ "$remove_images" = "true" ]; then
            log_info "  docker_compose down --rmi all"
        fi
        return 0
    fi
    
    # Stop and remove containers
    log_info "Stopping and removing containers..."
    if [ "$remove_images" = "true" ]; then
        MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" down --rmi all --volumes --remove-orphans
    else
        MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" down --volumes --remove-orphans
    fi
    
    log_info "✓ Cleanup complete"
}

# Function to manage models
manage_models() {
    local action="${1:-list}"
    
    case "$action" in
        "list")
            log_info "=== Available Models ==="
            local models_dir=$(ensure_models_dir)
            
            if [ -d "$models_dir" ] && [ "$(ls -A "$models_dir")" ]; then
                find "$models_dir" -name "*.bin" -type f | sort | while read -r model; do
                    local size=$(du -h "$model" | cut -f1)
                    local name=$(basename "$model")
                    log_info "  $name ($size)"
                done
            else
                log_warn "No models found in $models_dir"
                log_info "Models will be automatically downloaded when needed"
            fi
            ;;
        "download")
            local model_name="${2:-base.en}"
            local models_dir=$(ensure_models_dir)
            local model_file="$models_dir/ggml-${model_name}.bin"
            
            if [ -f "$model_file" ]; then
                local file_size=$(du -h "$model_file" | cut -f1)
                log_info "Model already exists: $model_file ($file_size)"
                return 0
            fi
            
            # Validate model name against available models
            local valid_model=false
            for available_model in "${AVAILABLE_MODELS[@]}"; do
                if [[ "$model_name" == "$available_model" ]]; then
                    valid_model=true
                    break
                fi
            done
            
            if [[ "$valid_model" == "false" ]]; then
                log_error "Invalid model name: $model_name"
                log_info "Available models:"
                printf '  %s\n' "${AVAILABLE_MODELS[@]}"
                return 1
            fi
            
            # Show download information
            log_info "Downloading model: $model_name"
            case "$model_name" in
                tiny*) log_info "📦 Size: ~39 MB (fastest, least accurate)" ;;
                base*) log_info "📦 Size: ~142 MB (good balance)" ;;
                small*) log_info "📦 Size: ~244 MB (better accuracy)" ;;
                medium*) log_info "📦 Size: ~769 MB (high accuracy)" ;;
                large*) log_info "📦 Size: ~1550 MB (best accuracy)" ;;
            esac
            
            local download_url="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-${model_name}.bin"
            log_info "🌐 URL: $download_url"
            
            # Create a temporary file for download
            local temp_file="${model_file}.tmp"
            
            # Download with progress and error handling
            log_info "🔄 Starting download..."
            if curl -L -f \
                --progress-bar \
                --connect-timeout 30 \
                --max-time 3600 \
                --retry 3 \
                --retry-delay 5 \
                --retry-connrefused \
                -o "$temp_file" \
                "$download_url"; then
                
                # Verify download completed successfully
                if [ -s "$temp_file" ]; then
                    mv "$temp_file" "$model_file"
                    local file_size=$(du -h "$model_file" | cut -f1)
                    log_info "✅ Model downloaded successfully: $model_file ($file_size)"
                else
                    log_error "❌ Downloaded file is empty"
                    rm -f "$temp_file"
                    return 1
                fi
            else
                log_error "❌ Failed to download model from $download_url"
                rm -f "$temp_file"
                log_error "💡 Troubleshooting:"
                log_error "   - Check internet connection"
                log_error "   - Verify model name is correct"
                log_error "   - Try again later (server might be busy)"
                return 1
            fi
            ;;
        *)
            log_error "Unknown models action: $action"
            log_info "Available actions: list, download"
            return 1
            ;;
    esac
}

# Function to test GPU
test_gpu() {
    log_info "=== GPU Test ==="
    
    local system_info
    system_info=$(detect_system)
    
    local gpu_available=$(echo "$system_info" | grep -o 'gpu_available:[^[:space:]]*' | cut -d: -f2)
    local gpu_type=$(echo "$system_info" | grep -o 'gpu_type:[^[:space:]]*' | cut -d: -f2)
    
    log_info "GPU Available: $gpu_available"
    log_info "GPU Type: $gpu_type"
    
    if [ "$gpu_available" = "true" ]; then
        if [ "$gpu_type" = "nvidia" ]; then
            log_info "NVIDIA GPU Details:"
            nvidia-smi 2>/dev/null || log_warn "nvidia-smi not available"
        fi
        
        # Test with container
        log_info "Testing GPU in container..."
        local image_info
        image_info=$(choose_image "gpu" "")
        local image=$(echo "$image_info" | grep -o 'image:[^[:space:]]*' | cut -d: -f2-)
        
        if check_image "$image"; then
            docker run --rm --gpus all "$image" gpu-test
        else
            log_warn "GPU image not built, run: $0 build gpu"
        fi
    else
        log_info "No GPU detected"
    fi
}

# Main function
main() {
    local command="${1:-start}"
    shift || true
    
    case "$command" in
        "start")
            start_server "$@"
            ;;
        "stop")
            stop_server "$@"
            ;;
        "restart")
            stop_server
            sleep 2
            start_server "$@"
            ;;
        "logs")
            show_logs "$@"
            ;;
        "status")
            show_status "$@"
            ;;
        "shell")
            open_shell "$@"
            ;;
        "clean")
            clean_up "$@"
            ;;
        "build")
            "$SCRIPT_DIR/build-docker.sh" "$@"
            ;;
        "models")
            manage_models "$@"
            ;;
        "gpu-test")
            test_gpu "$@"
            ;;
        "setup-db")
            shift
            "$SCRIPT_DIR/setup-db.sh" "$@"
            ;;
        "compose")
            shift
            MODEL_NAME="$DEFAULT_MODEL" docker_compose "${COMPOSE_PROFILE_ARGS[@]}" "$@"
            ;;
        "help"|"--help"|"-h")
            show_help
            ;;
        *)
            log_error "Unknown command: $command"
            show_help
            exit 1
            ;;
    esac
}

# Parse global options
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help|-h)
            show_help
            exit 0
            ;;
        *)
            break
            ;;
    esac
done

# Execute main function
cd "$SCRIPT_DIR"
main "$@"