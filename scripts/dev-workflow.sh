#!/bin/bash
#
# Development Workflow Script
# 
# Provides hot-reload TUI development with automated testing and live cluster integration
# Supports multiple development modes for efficient TUI development

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration
DEV_CLUSTER_PORT=9000
DEV_DATA_DIR="$PROJECT_ROOT/dev-data"
TUI_PID_FILE="$PROJECT_ROOT/.tui-dev.pid"
CLUSTER_PID_FILE="$PROJECT_ROOT/.cluster-dev.pid"
WATCH_PID_FILE="$PROJECT_ROOT/.watch-dev.pid"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m'

log() {
    echo -e "${BLUE}[DEV]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

info() {
    echo -e "${PURPLE}[INFO]${NC} $*"
}

cleanup() {
    log "Cleaning up development environment..."
    
    # Stop watch process
    if [[ -f "$WATCH_PID_FILE" ]]; then
        local watch_pid=$(cat "$WATCH_PID_FILE")
        if kill -0 "$watch_pid" 2>/dev/null; then
            kill "$watch_pid" 2>/dev/null || true
        fi
        rm -f "$WATCH_PID_FILE"
    fi
    
    # Stop TUI
    if [[ -f "$TUI_PID_FILE" ]]; then
        local tui_pid=$(cat "$TUI_PID_FILE")
        if kill -0 "$tui_pid" 2>/dev/null; then
            kill "$tui_pid" 2>/dev/null || true
        fi
        rm -f "$TUI_PID_FILE"
    fi
    
    # Stop cluster
    if [[ -f "$CLUSTER_PID_FILE" ]]; then
        local cluster_pid=$(cat "$CLUSTER_PID_FILE")
        if kill -0 "$cluster_pid" 2>/dev/null; then
            kill "$cluster_pid" 2>/dev/null || true
        fi
        rm -f "$CLUSTER_PID_FILE"
    fi
    
    # Clean up dev data
    if [[ -d "$DEV_DATA_DIR" ]]; then
        rm -rf "$DEV_DATA_DIR"
    fi
    
    log "Cleanup completed"
}

trap cleanup EXIT

check_dependencies() {
    local missing=()
    
    if ! command -v cargo >/dev/null 2>&1; then
        missing+=("cargo")
    fi
    
    if ! command -v watchexec >/dev/null 2>&1; then
        missing+=("watchexec")
    fi
    
    if [[ ${#missing[@]} -gt 0 ]]; then
        error "Missing dependencies: ${missing[*]}"
        error "Install with: cargo install watchexec-cli"
        exit 1
    fi
}

build_project() {
    log "Building project..."
    cd "$PROJECT_ROOT"
    
    if cargo build; then
        success "Build successful"
        return 0
    else
        error "Build failed"
        return 1
    fi
}

start_dev_cluster() {
    log "Starting development cluster..."
    
    mkdir -p "$DEV_DATA_DIR"
    
    # Start single-node cluster for development
    cd "$PROJECT_ROOT"
    cargo run --release -- node \
        --id 1 \
        --bind "127.0.0.1:$DEV_CLUSTER_PORT" \
        --data-dir "$DEV_DATA_DIR/node-1" \
        > "$DEV_DATA_DIR/cluster.log" 2>&1 &
    
    local cluster_pid=$!
    echo "$cluster_pid" > "$CLUSTER_PID_FILE"
    
    # Wait for cluster to start
    log "Waiting for cluster to initialize..."
    local attempts=0
    while ! nc -z 127.0.0.1 "$DEV_CLUSTER_PORT" 2>/dev/null; do
        sleep 1
        attempts=$((attempts + 1))
        if [[ $attempts -gt 20 ]]; then
            error "Development cluster failed to start"
            return 1
        fi
    done
    
    success "Development cluster started on port $DEV_CLUSTER_PORT"
    
    # Create some test VMs for development
    sleep 2
    create_test_data
}

create_test_data() {
    log "Creating test data..."
    
    # Create test VMs
    cargo run --release -- vm create \
        --name "dev-web-server" \
        --vcpus 2 \
        --memory 1024 \
        --address "127.0.0.1:$DEV_CLUSTER_PORT" 2>/dev/null || true
    
    cargo run --release -- vm create \
        --name "dev-database" \
        --vcpus 4 \
        --memory 2048 \
        --address "127.0.0.1:$DEV_CLUSTER_PORT" 2>/dev/null || true
    
    cargo run --release -- vm create \
        --name "dev-worker" \
        --vcpus 1 \
        --memory 512 \
        --address "127.0.0.1:$DEV_CLUSTER_PORT" 2>/dev/null || true
    
    success "Test data created"
}

start_tui() {
    log "Starting TUI..."
    
    cd "$PROJECT_ROOT"
    cargo run --release -- tui > "$DEV_DATA_DIR/tui.log" 2>&1 &
    
    local tui_pid=$!
    echo "$tui_pid" > "$TUI_PID_FILE"
    
    success "TUI started (PID: $tui_pid)"
    info "TUI log: $DEV_DATA_DIR/tui.log"
}

restart_tui() {
    log "Restarting TUI..."
    
    # Stop existing TUI
    if [[ -f "$TUI_PID_FILE" ]]; then
        local tui_pid=$(cat "$TUI_PID_FILE")
        if kill -0 "$tui_pid" 2>/dev/null; then
            kill "$tui_pid" 2>/dev/null || true
            sleep 1
        fi
        rm -f "$TUI_PID_FILE"
    fi
    
    # Rebuild and restart
    if build_project; then
        start_tui
        success "TUI restarted successfully"
    else
        error "Failed to restart TUI due to build error"
    fi
}

start_hot_reload() {
    log "Starting hot-reload file watcher..."
    
    # Watch for changes in TUI-related files
    watchexec \
        --restart \
        --delay-run 1000 \
        --watch "$PROJECT_ROOT/blixard/src/tui" \
        --watch "$PROJECT_ROOT/blixard/src/main.rs" \
        --watch "$PROJECT_ROOT/blixard/Cargo.toml" \
        --exts rs,toml \
        --ignore "target/*" \
        --ignore "*.log" \
        -- bash -c "echo 'File change detected, rebuilding...'; $0 restart-tui" &
    
    local watch_pid=$!
    echo "$watch_pid" > "$WATCH_PID_FILE"
    
    success "Hot-reload watcher started (PID: $watch_pid)"
    info "Watching: blixard/src/tui/, blixard/src/main.rs, blixard/Cargo.toml"
}

run_automated_tests() {
    log "Running automated TUI tests..."
    
    cd "$PROJECT_ROOT"
    
    # Run TUI integration tests
    if cargo test --test tui_integration_tests; then
        success "TUI integration tests passed"
    else
        error "TUI integration tests failed"
    fi
    
    # Run TUI component tests
    if cargo test tui_; then
        success "TUI component tests passed"
    else
        warn "Some TUI component tests failed"
    fi
}

start_test_watcher() {
    log "Starting automated test watcher..."
    
    # Watch for changes and run tests
    watchexec \
        --restart \
        --delay-run 2000 \
        --watch "$PROJECT_ROOT/blixard/src/tui" \
        --watch "$PROJECT_ROOT/tests" \
        --exts rs \
        --ignore "target/*" \
        --ignore "*.log" \
        -- bash -c "$0 run-tests" &
    
    local test_watch_pid=$!
    
    success "Test watcher started (PID: $test_watch_pid)"
}

show_dev_status() {
    echo
    info "=== Development Environment Status ==="
    
    # Check cluster
    if [[ -f "$CLUSTER_PID_FILE" ]]; then
        local cluster_pid=$(cat "$CLUSTER_PID_FILE")
        if kill -0 "$cluster_pid" 2>/dev/null; then
            success "✓ Development cluster running (PID: $cluster_pid)"
            success "  Connect at: 127.0.0.1:$DEV_CLUSTER_PORT"
        else
            error "✗ Development cluster not running"
        fi
    else
        error "✗ Development cluster not started"
    fi
    
    # Check TUI
    if [[ -f "$TUI_PID_FILE" ]]; then
        local tui_pid=$(cat "$TUI_PID_FILE")
        if kill -0 "$tui_pid" 2>/dev/null; then
            success "✓ TUI running (PID: $tui_pid)"
        else
            error "✗ TUI not running"
        fi
    else
        error "✗ TUI not started"
    fi
    
    # Check hot-reload
    if [[ -f "$WATCH_PID_FILE" ]]; then
        local watch_pid=$(cat "$WATCH_PID_FILE")
        if kill -0 "$watch_pid" 2>/dev/null; then
            success "✓ Hot-reload watcher active (PID: $watch_pid)"
        else
            error "✗ Hot-reload watcher not running"
        fi
    else
        error "✗ Hot-reload not started"
    fi
    
    echo
    info "=== Available Commands ==="
    info "  $0 status           - Show this status"
    info "  $0 restart-tui      - Restart TUI only"
    info "  $0 run-tests        - Run automated tests"
    info "  $0 logs             - Show logs"
    info "  $0 stop             - Stop development environment"
    echo
}

show_logs() {
    log "Development environment logs:"
    echo
    
    if [[ -f "$DEV_DATA_DIR/cluster.log" ]]; then
        info "=== Cluster Logs (last 20 lines) ==="
        tail -20 "$DEV_DATA_DIR/cluster.log"
        echo
    fi
    
    if [[ -f "$DEV_DATA_DIR/tui.log" ]]; then
        info "=== TUI Logs (last 20 lines) ==="
        tail -20 "$DEV_DATA_DIR/tui.log"
        echo
    fi
}

show_usage() {
    cat << EOF
Usage: $0 [COMMAND]

Development workflow for Blixard TUI with hot-reload and automated testing.

COMMANDS:
    start           Start full development environment (cluster + TUI + hot-reload)
    start-cluster   Start development cluster only
    start-tui       Start TUI only
    restart-tui     Restart TUI (used by hot-reload)
    hot-reload      Start hot-reload file watcher
    run-tests       Run automated TUI tests
    test-watch      Start automated test watcher
    status          Show development environment status
    logs            Show recent logs
    stop            Stop all development processes
    help            Show this help message

DEVELOPMENT MODES:
    1. Full Development:
       $0 start
       - Starts cluster, TUI, and hot-reload
       - Changes to TUI code automatically restart TUI
       - Includes test data for development

    2. TUI Development Only:
       $0 start-cluster
       $0 hot-reload
       - Start cluster and hot-reload without TUI
       - Manually start TUI when needed

    3. Testing Mode:
       $0 run-tests
       $0 test-watch
       - Run tests manually or with file watcher

HOT-RELOAD FEATURES:
    • Automatic TUI restart on code changes
    • Preserves cluster state during restarts
    • Fast incremental rebuilds
    • Live test execution
    • Development cluster with test data

EXAMPLES:
    $0 start                    # Full development environment
    $0 status                   # Check what's running
    $0 logs                     # View recent logs
    $0 restart-tui              # Manual TUI restart
    $0 run-tests                # Run TUI tests

EOF
}

main() {
    case "${1:-help}" in
        start)
            check_dependencies
            build_project
            start_dev_cluster
            start_tui
            start_hot_reload
            show_dev_status
            
            info "Development environment ready!"
            info "• TUI will automatically restart on code changes"
            info "• Cluster is running with test data"
            info "• Press Ctrl+C to stop everything"
            
            # Keep script running
            while true; do
                sleep 10
                if ! kill -0 "$(cat "$CLUSTER_PID_FILE" 2>/dev/null)" 2>/dev/null; then
                    error "Cluster stopped unexpectedly"
                    break
                fi
            done
            ;;
        start-cluster)
            check_dependencies
            build_project
            start_dev_cluster
            show_dev_status
            ;;
        start-tui)
            build_project
            start_tui
            ;;
        restart-tui)
            restart_tui
            ;;
        hot-reload)
            start_hot_reload
            info "Hot-reload active. Make changes to TUI code to see automatic restarts."
            ;;
        run-tests)
            run_automated_tests
            ;;
        test-watch)
            start_test_watcher
            info "Test watcher active. Tests will run automatically on code changes."
            ;;
        status)
            show_dev_status
            ;;
        logs)
            show_logs
            ;;
        stop)
            cleanup
            success "Development environment stopped"
            exit 0
            ;;
        help|--help|-h)
            show_usage
            ;;
        *)
            error "Unknown command: $1"
            show_usage
            exit 1
            ;;
    esac
}

main "$@"