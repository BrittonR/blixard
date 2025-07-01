#!/bin/bash
# Test Iroh transport locally with multiple nodes
# This script sets up a local test cluster to validate Iroh functionality

set -euo pipefail

# Configuration
BLIXARD_BIN="${BLIXARD_BIN:-cargo run --}"
BASE_PORT=7000
NUM_NODES=3
DATA_DIR="/tmp/blixard-iroh-test"
LOG_DIR="/tmp/blixard-logs"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=== Blixard Iroh Local Test Setup ==="
echo ""

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    
    # Kill all node processes
    for i in $(seq 1 $NUM_NODES); do
        if [[ -f "$DATA_DIR/node$i.pid" ]]; then
            pid=$(cat "$DATA_DIR/node$i.pid")
            kill $pid 2>/dev/null || true
            rm -f "$DATA_DIR/node$i.pid"
        fi
    done
    
    # Remove data directories
    rm -rf "$DATA_DIR"
    rm -rf "$LOG_DIR"
    
    echo -e "${GREEN}Cleanup complete${NC}"
}

# Set up trap for cleanup
trap cleanup EXIT

# Create directories
mkdir -p "$DATA_DIR" "$LOG_DIR"

# Create test configuration files
create_node_config() {
    local node_id=$1
    local port=$((BASE_PORT + node_id))
    local config_file="$DATA_DIR/node$node_id.toml"
    
    cat > "$config_file" << EOF
# Node $node_id configuration
[node]
id = $node_id
bind_addr = "127.0.0.1:$port"
data_dir = "$DATA_DIR/node$node_id"

[transport]
mode = "iroh"

[transport.iroh]
# Use local relay for testing
relay_mode = "auto"
stun_servers = ["stun.l.google.com:19302"]

# Performance settings for testing
max_connections = 50
connection_pool_size = 5
batch_window_ms = 10

# Logging
[logging]
level = "debug"
format = "json"
EOF

    if [[ $node_id -gt 1 ]]; then
        echo "join_addr = \"127.0.0.1:$((BASE_PORT + 1))\"" >> "$config_file"
    fi
    
    echo "$config_file"
}

# Start a node
start_node() {
    local node_id=$1
    local config_file=$2
    local log_file="$LOG_DIR/node$node_id.log"
    
    echo -e "Starting node $node_id..."
    
    # Start the node in background
    RUST_LOG=blixard=debug,iroh=debug \
    $BLIXARD_BIN node \
        --config "$config_file" \
        > "$log_file" 2>&1 &
    
    local pid=$!
    echo $pid > "$DATA_DIR/node$node_id.pid"
    
    # Wait for node to start
    local attempts=0
    while ! grep -q "Node started" "$log_file" 2>/dev/null; do
        if [[ $attempts -gt 30 ]]; then
            echo -e "${RED}Node $node_id failed to start${NC}"
            tail -20 "$log_file"
            return 1
        fi
        sleep 1
        ((attempts++))
    done
    
    echo -e "${GREEN}Node $node_id started (PID: $pid)${NC}"
}

# Test node connectivity
test_connectivity() {
    echo -e "\n${YELLOW}Testing connectivity...${NC}"
    
    # Give nodes time to discover each other
    sleep 5
    
    # Check for P2P connections established
    echo -e "\n${YELLOW}Checking P2P connections...${NC}"
    for i in $(seq 1 $NUM_NODES); do
        local log_file="$LOG_DIR/node$i.log"
        echo -e "\nNode $i P2P connections:"
        
        # Check for successful P2P connections
        if grep -q "Successfully connected to P2P peer" "$log_file" 2>/dev/null; then
            grep "Successfully connected to P2P peer" "$log_file" | tail -5
            echo -e "${GREEN}✓ P2P connections established${NC}"
        else
            echo -e "${RED}✗ No P2P connections found${NC}"
        fi
        
        # Check for connection attempts
        echo "  Connection attempts:"
        grep -E "(Establishing P2P connection|Attempting to connect to P2P peer)" "$log_file" 2>/dev/null | tail -5 || echo "    None found"
        
        # Check for failures
        if grep -q "Failed to establish P2P connection" "$log_file" 2>/dev/null; then
            echo -e "  ${RED}Connection failures:${NC}"
            grep "Failed to establish P2P connection" "$log_file" | tail -3
        fi
    done
    
    # Test health checks between nodes
    echo -e "\n${YELLOW}Testing node connectivity...${NC}"
    for i in $(seq 1 $NUM_NODES); do
        for j in $(seq 1 $NUM_NODES); do
            if [[ $i -ne $j ]]; then
                echo -n "Testing node $i -> node $j: "
                
                # Check if connection exists in logs
                if grep -q "Successfully connected to P2P peer $j" "$LOG_DIR/node$i.log" 2>/dev/null; then
                    echo -e "${GREEN}OK (P2P connected)${NC}"
                else
                    echo -e "${YELLOW}Not connected${NC}"
                fi
            fi
        done
    done
}

# Monitor metrics
monitor_metrics() {
    echo -e "\n${YELLOW}Monitoring metrics (press Ctrl+C to stop)...${NC}"
    
    while true; do
        clear
        echo "=== Iroh Transport Metrics ==="
        echo ""
        
        for i in $(seq 1 $NUM_NODES); do
            local log_file="$LOG_DIR/node$i.log"
            
            echo "Node $i:"
            # Extract metrics from logs
            if [[ -f "$log_file" ]]; then
                echo -n "  P2P Connections: "
                grep -c "Successfully connected to P2P peer" "$log_file" 2>/dev/null || echo "0"
                
                echo -n "  Connection Attempts: "
                grep -c "Establishing P2P connection" "$log_file" 2>/dev/null || echo "0"
                
                echo -n "  Connection Failures: "
                grep -c "Failed to establish P2P connection" "$log_file" 2>/dev/null || echo "0"
                
                echo -n "  Messages sent: "
                grep -c "message sent" "$log_file" 2>/dev/null || echo "0"
                
                echo -n "  Errors: "
                grep -c "ERROR" "$log_file" 2>/dev/null || echo "0"
                
                # Show connected peers
                local connected_peers=$(grep "Successfully connected to P2P peer" "$log_file" 2>/dev/null | sed 's/.*peer //' | sort -u | tr '\n' ',' | sed 's/,$//')
                if [[ -n "$connected_peers" ]]; then
                    echo "  Connected to peers: $connected_peers"
                fi
            fi
            echo ""
        done
        
        sleep 2
    done
}

# Main execution
main() {
    echo "Setting up $NUM_NODES node cluster with Iroh transport..."
    echo ""
    
    # Create configs and start nodes
    for i in $(seq 1 $NUM_NODES); do
        config_file=$(create_node_config $i)
        start_node $i "$config_file"
        
        # Stagger node starts
        if [[ $i -lt $NUM_NODES ]]; then
            sleep 2
        fi
    done
    
    echo -e "\n${GREEN}All nodes started!${NC}"
    echo ""
    echo "Log files:"
    for i in $(seq 1 $NUM_NODES); do
        echo "  Node $i: $LOG_DIR/node$i.log"
    done
    echo ""
    
    # Test connectivity
    test_connectivity
    
    # Run specific tests
    echo -e "\n${YELLOW}Running Iroh benchmarks...${NC}"
    cargo run --example iroh_raft_benchmark
    
    echo -e "\n${YELLOW}Running network tests...${NC}"
    cargo run --example iroh_network_test
    
    # Offer monitoring
    echo -e "\n${GREEN}Tests complete!${NC}"
    echo ""
    echo "Options:"
    echo "  1) Monitor metrics"
    echo "  2) View logs"
    echo "  3) Run more tests"
    echo "  4) Exit"
    echo ""
    read -p "Select option (1-4): " option
    
    case $option in
        1)
            monitor_metrics
            ;;
        2)
            echo "Showing recent logs..."
            for i in $(seq 1 $NUM_NODES); do
                echo -e "\n=== Node $i ==="
                tail -20 "$LOG_DIR/node$i.log"
            done
            ;;
        3)
            echo "Running additional tests..."
            cargo test --features iroh-transport
            ;;
        4)
            echo "Exiting..."
            ;;
    esac
}

# Run main
main