#!/bin/bash

# Database Setup Script for Meeting App
# Handles existing database discovery and migration

set -e

# Configuration
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
DEFAULT_DB_PATH="/opt/homebrew/Cellar/meetily-backend/0.0.4/backend/meeting_minutes.db"
DOCKER_DB_DIR="$SCRIPT_DIR/data"
DOCKER_DB_PATH="$DOCKER_DB_DIR/meeting_minutes.db"

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

# Ensure data directory exists
ensure_data_directory() {
    if [ ! -d "$DOCKER_DB_DIR" ]; then
        log_info "Creating data directory..."
        mkdir -p "$DOCKER_DB_DIR"
        chmod 755 "$DOCKER_DB_DIR"
        log_info "✓ Data directory created at: $DOCKER_DB_DIR"
    fi
}

show_help() {
    cat << EOF
Meeting App Database Setup Script

This script helps you set up the database for the Meeting App by:
1. Checking for existing database from previous installations
2. Copying/migrating existing database if found
3. Setting up fresh database for first-time installations

Usage: $0 [OPTIONS]

OPTIONS:
  --db-path PATH       Specify custom database path to migrate from
  --fresh              Skip existing database search, create fresh database
  --auto               Auto-detect and migrate without prompts (if found)
  -h, --help           Show this help

Examples:
  # Interactive setup (recommended)
  $0
  
  # Migrate from custom path
  $0 --db-path /path/to/meeting_minutes.db
  
  # Fresh installation
  $0 --fresh
  
  # Auto-detect and migrate
  $0 --auto

EOF
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
    
    log_info "Database Information:"
    echo "  Path: $db_path"
    echo "  Size: $(du -h "$db_path" | cut -f1)"
    echo "  Modified: $(stat -f "%Sm" "$db_path" 2>/dev/null || stat -c "%y" "$db_path" 2>/dev/null || echo "Unknown")"
    
    # Try to get table counts
    local meetings_count=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM meetings;" 2>/dev/null || echo "0")
    local transcripts_count=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM transcripts;" 2>/dev/null || echo "0")
    
    echo "  Meetings: $meetings_count"
    echo "  Transcripts: $transcripts_count"
}

# Function to copy database
copy_database() {
    local source_path="$1"
    local dest_path="$2"
    
    log_info "Copying database from $source_path to $dest_path"
    
    # Create destination directory if it doesn't exist
    mkdir -p "$(dirname "$dest_path")"
    
    # Copy the database file
    cp "$source_path" "$dest_path"
    
    # Set proper permissions
    chmod 644 "$dest_path"
    
    log_info "✓ Database copied successfully"
}

# Function to find existing databases
find_existing_databases() {
    local found_dbs=()
    
    # Check default location
    if check_database "$DEFAULT_DB_PATH"; then
        found_dbs+=("$DEFAULT_DB_PATH")
    fi
    
    # Check other common locations
    local common_paths=(
        "/opt/homebrew/Cellar/meetily-backend/*/backend/meeting_minutes.db"
        "$HOME/.meetily/meeting_minutes.db"
        "$HOME/Documents/meetily/meeting_minutes.db"
        "$HOME/Desktop/meeting_minutes.db"
        "./meeting_minutes.db"
    )
    
    for pattern in "${common_paths[@]}"; do
        for path in $pattern; do
            if [[ "$path" != "$DEFAULT_DB_PATH" ]] && check_database "$path"; then
                found_dbs+=("$path")
            fi
        done
    done
    
    printf '%s\n' "${found_dbs[@]}"
}

# Interactive database selection
interactive_setup() {
    echo
    log_info "=== Meeting App Database Setup ==="
    echo
    
    log_info "Searching for existing databases..."
    local found_dbs=($(find_existing_databases))
    
    if [ ${#found_dbs[@]} -eq 0 ]; then
        log_info "No existing databases found."
        echo
        echo "Options:"
        echo "1) First-time installation (create fresh database)"
        echo "2) I have an existing database at a custom location"
        echo "3) Exit"
        echo
        read -p "Please choose an option (1-3): " choice
        
        case $choice in
            1)
                log_info "Setting up fresh database for first-time installation"
                setup_fresh_database
                ;;
            2)
                read -p "Enter the full path to your existing database: " custom_path
                if check_database "$custom_path"; then
                    get_database_info "$custom_path"
                    echo
                    read -p "Use this database? (y/N): " confirm
                    if [[ $confirm =~ ^[Yy]$ ]]; then
                        copy_database "$custom_path" "$DOCKER_DB_PATH"
                    else
                        log_info "Database setup cancelled"
                        exit 0
                    fi
                else
                    log_error "Invalid database file: $custom_path"
                    exit 1
                fi
                ;;
            3)
                log_info "Setup cancelled"
                exit 0
                ;;
            *)
                log_error "Invalid choice"
                exit 1
                ;;
        esac
    else
        log_info "Found ${#found_dbs[@]} existing database(s):"
        echo
        
        for i in "${!found_dbs[@]}"; do
            echo "$((i+1))) ${found_dbs[i]}"
        done
        echo "$((${#found_dbs[@]}+1))) Use custom path"
        echo "$((${#found_dbs[@]}+2))) Fresh installation"
        echo "$((${#found_dbs[@]}+3))) Exit"
        echo
        
        read -p "Please choose an option: " choice
        
        if [[ $choice -ge 1 && $choice -le ${#found_dbs[@]} ]]; then
            local selected_db="${found_dbs[$((choice-1))]}"
            echo
            get_database_info "$selected_db"
            echo
            read -p "Use this database? (Y/n): " confirm
            if [[ ! $confirm =~ ^[Nn]$ ]]; then
                copy_database "$selected_db" "$DOCKER_DB_PATH"
            else
                log_info "Database setup cancelled"
                exit 0
            fi
        elif [[ $choice -eq $((${#found_dbs[@]}+1)) ]]; then
            read -p "Enter the full path to your existing database: " custom_path
            if check_database "$custom_path"; then
                get_database_info "$custom_path"
                echo
                read -p "Use this database? (y/N): " confirm
                if [[ $confirm =~ ^[Yy]$ ]]; then
                    copy_database "$custom_path" "$DOCKER_DB_PATH"
                else
                    log_info "Database setup cancelled"
                    exit 0
                fi
            else
                log_error "Invalid database file: $custom_path"
                exit 1
            fi
        elif [[ $choice -eq $((${#found_dbs[@]}+2)) ]]; then
            log_info "Setting up fresh database for first-time installation"
            setup_fresh_database
        elif [[ $choice -eq $((${#found_dbs[@]}+3)) ]]; then
            log_info "Setup cancelled"
            exit 0
        else
            log_error "Invalid choice"
            exit 1
        fi
    fi
}

# Function to setup fresh database
setup_fresh_database() {
    # Create data directory
    mkdir -p "$DOCKER_DB_DIR"
    
    # Remove existing database if any
    if [ -f "$DOCKER_DB_PATH" ]; then
        rm "$DOCKER_DB_PATH"
    fi
    
    # Create empty database file with proper permissions
    touch "$DOCKER_DB_PATH"
    chmod 644 "$DOCKER_DB_PATH"
    
    log_info "✓ Fresh database created at: $DOCKER_DB_PATH"
    log_info "The application will initialize the database schema on first run"
}

# Auto setup function
auto_setup() {
    log_info "Auto-detecting existing databases..."
    
    if check_database "$DEFAULT_DB_PATH"; then
        log_info "Found database at default location: $DEFAULT_DB_PATH"
        get_database_info "$DEFAULT_DB_PATH"
        copy_database "$DEFAULT_DB_PATH" "$DOCKER_DB_PATH"
    else
        local found_dbs=($(find_existing_databases))
        if [ ${#found_dbs[@]} -gt 0 ]; then
            log_info "Found database: ${found_dbs[0]}"
            get_database_info "${found_dbs[0]}"
            copy_database "${found_dbs[0]}" "$DOCKER_DB_PATH"
        else
            log_info "No existing databases found, setting up fresh installation"
            setup_fresh_database
        fi
    fi
}

# Main function
main() {
    # Ensure data directory exists first
    ensure_data_directory
    
    local custom_db_path=""
    local fresh_install=false
    local auto_mode=false
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --db-path)
                custom_db_path="$2"
                shift 2
                ;;
            --fresh)
                fresh_install=true
                shift
                ;;
            --auto)
                auto_mode=true
                shift
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # For fresh install, sqlite3 is not required
    if [ "$fresh_install" = true ]; then
        setup_fresh_database
    else
        # Check if sqlite3 is available for database operations
        if ! command -v sqlite3 >/dev/null 2>&1; then
            log_error "sqlite3 is required for database operations but not installed"
            log_error "Please install sqlite3 or use --fresh for a fresh installation"
            exit 1
        fi
    fi
    
    if [ -n "$custom_db_path" ]; then
        if check_database "$custom_db_path"; then
            get_database_info "$custom_db_path"
            copy_database "$custom_db_path" "$DOCKER_DB_PATH"
        else
            log_error "Invalid database file: $custom_db_path"
            exit 1
        fi
    elif [ "$auto_mode" = true ]; then
        auto_setup
    else
        interactive_setup
    fi
    
    log_info "=== Database Setup Complete ==="
    log_info "Database location: $DOCKER_DB_PATH"
    log_info "You can now start the services with: ./run-docker.sh compose up -d"
}

# Execute main function
main "$@"