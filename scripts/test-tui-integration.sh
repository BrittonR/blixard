#!/bin/bash
#
# TUI Integration Test Script
# 
# Tests the TUI against real Blixard clusters for end-to-end validation
# Covers cluster discovery, VM management, and node operations

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Test configuration
TEST_CLUSTER_BASE_PORT=8100
TEST_CLUSTER_SIZE=3
TEST_DATA_DIR="$PROJECT_ROOT/test-tui-data"
TUI_LOG_FILE="$PROJECT_ROOT/tui-test.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[TUI-TEST]${NC} $*" | tee -a "$TUI_LOG_FILE"
}

warn() {
    echo -e "${YELLOW}[TUI-WARN]${NC} $*" | tee -a "$TUI_LOG_FILE"
}

error() {
    echo -e "${RED}[TUI-ERROR]${NC} $*" | tee -a "$TUI_LOG_FILE"
}

success() {
    echo -e "${GREEN}[TUI-SUCCESS]${NC} $*" | tee -a "$TUI_LOG_FILE"
}

cleanup() {
    log "Cleaning up test resources..."
    
    # Kill any running test nodes
    for i in $(seq 1 $TEST_CLUSTER_SIZE); do
        local port=$((TEST_CLUSTER_BASE_PORT + i))
        local pid_file="$TEST_DATA_DIR/node-$i.pid"
        
        if [[ -f "$pid_file" ]]; then
            local pid=$(cat "$pid_file")
            if kill -0 "$pid" 2>/dev/null; then
                log "Stopping test node $i (PID: $pid)"
                kill "$pid" || true
                wait "$pid" 2>/dev/null || true
            fi
            rm -f "$pid_file"
        fi
    done
    
    # Clean up data directories
    if [[ -d "$TEST_DATA_DIR" ]]; then
        rm -rf "$TEST_DATA_DIR"
    fi
    
    log "Cleanup completed"
}

# Set up cleanup trap
trap cleanup EXIT

setup_test_cluster() {
    log "Setting up test cluster with $TEST_CLUSTER_SIZE nodes..."
    
    mkdir -p "$TEST_DATA_DIR"
    rm -f "$TUI_LOG_FILE"
    
    # Build the project first
    log "Building blixard..."
    cd "$PROJECT_ROOT"
    cargo build --release
    
    local bootstrap_port=$((TEST_CLUSTER_BASE_PORT + 1))
    local bootstrap_data_dir="$TEST_DATA_DIR/node-1"
    
    # Start bootstrap node
    log "Starting bootstrap node on port $bootstrap_port..."
    mkdir -p "$bootstrap_data_dir"
    
    cargo run --release -- node \
        --id 1 \
        --bind "127.0.0.1:$bootstrap_port" \
        --data-dir "$bootstrap_data_dir" \
        > "$TEST_DATA_DIR/node-1.log" 2>&1 &
    
    local bootstrap_pid=$!
    echo "$bootstrap_pid" > "$TEST_DATA_DIR/node-1.pid"
    
    # Wait for bootstrap node to start
    log "Waiting for bootstrap node to initialize..."
    local attempts=0
    while ! nc -z 127.0.0.1 "$bootstrap_port" 2>/dev/null; do
        sleep 1
        attempts=$((attempts + 1))
        if [[ $attempts -gt 30 ]]; then
            error "Bootstrap node failed to start within 30 seconds"
            return 1
        fi
    done
    
    success "Bootstrap node started successfully"
    
    # Start additional nodes
    for i in $(seq 2 $TEST_CLUSTER_SIZE); do
        local port=$((TEST_CLUSTER_BASE_PORT + i))
        local data_dir="$TEST_DATA_DIR/node-$i"
        
        log "Starting node $i on port $port..."
        mkdir -p "$data_dir"
        
        cargo run --release -- node \
            --id "$i" \
            --bind "127.0.0.1:$port" \
            --data-dir "$data_dir" \
            --peers "127.0.0.1:$bootstrap_port" \
            > "$TEST_DATA_DIR/node-$i.log" 2>&1 &
        
        local pid=$!
        echo "$pid" > "$TEST_DATA_DIR/node-$i.pid"
        
        # Wait for node to start
        local attempts=0
        while ! nc -z 127.0.0.1 "$port" 2>/dev/null; do
            sleep 1
            attempts=$((attempts + 1))
            if [[ $attempts -gt 30 ]]; then
                error "Node $i failed to start within 30 seconds"
                return 1
            fi
        done
        
        success "Node $i started successfully"
        sleep 2 # Give time for cluster formation
    done
    
    # Verify cluster formation
    log "Verifying cluster formation..."
    sleep 5
    
    local cluster_status
    if cluster_status=$(timeout 10 cargo run --release -- status --address "127.0.0.1:$bootstrap_port" 2>/dev/null); then
        log "Cluster status: $cluster_status"
        success "Test cluster is ready!"
        return 0
    else
        error "Failed to verify cluster status"
        return 1
    fi
}

run_integration_tests() {
    log "Running TUI integration tests..."
    
    # Run our Rust integration tests
    log "Running Rust TUI integration tests..."
    if cargo test --test tui_integration_tests; then
        success "Rust TUI integration tests passed!"
    else
        error "Rust TUI integration tests failed"
        return 1
    fi
}

show_usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Test the Blixard TUI with real cluster operations.

OPTIONS:
    --cluster-size N    Number of nodes in test cluster (default: 3)
    --base-port PORT    Base port for test cluster (default: 8100)
    --keep-cluster      Keep test cluster running after tests
    --tui-only          Only run TUI tests, don't set up cluster
    --help              Show this help message

EXAMPLES:
    $0                                    # Run full TUI integration test
    $0 --cluster-size 5                   # Test with 5-node cluster
    $0 --keep-cluster                     # Keep cluster running for manual testing
    $0 --tui-only                         # Only test TUI components

EOF
}

main() {
    local keep_cluster=false
    local tui_only=false
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --cluster-size)
                TEST_CLUSTER_SIZE="$2"
                shift 2
                ;;
            --base-port)
                TEST_CLUSTER_BASE_PORT="$2"
                shift 2
                ;;
            --keep-cluster)
                keep_cluster=true
                shift
                ;;
            --tui-only)
                tui_only=true
                shift
                ;;
            --help)
                show_usage
                exit 0
                ;;
            *)
                error "Unknown argument: $1"
                show_usage
                exit 1
                ;;
        esac
    done
    
    log "Starting TUI integration tests..."
    log "Cluster size: $TEST_CLUSTER_SIZE nodes"
    log "Base port: $TEST_CLUSTER_BASE_PORT"
    log "Data directory: $TEST_DATA_DIR"
    
    if [[ "$tui_only" == "true" ]]; then
        log "Running TUI-only tests..."
        run_integration_tests
    else
        # Set up test cluster
        if ! setup_test_cluster; then
            error "Failed to set up test cluster"
            exit 1
        fi
        
        # Run integration tests
        run_integration_tests
        
        if [[ "$keep_cluster" == "true" ]]; then
            success "Test cluster is running. Connect with:"
            echo "  cargo run --release -- tui"
            echo "  Then use Shift+C to discover cluster at 127.0.0.1:$((TEST_CLUSTER_BASE_PORT + 1))"
            echo ""
            echo "To stop the cluster, run: pkill -f 'blixard.*node'"
            echo "Or clean up with: rm -rf $TEST_DATA_DIR"
            
            # Disable cleanup trap
            trap - EXIT
        fi
    fi
    
    success "TUI integration tests completed successfully!"
}

# Check dependencies
if ! command -v nc >/dev/null 2>&1; then
    error "netcat (nc) is required but not installed"
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    error "cargo is required but not installed"
    exit 1
fi

main "$@"