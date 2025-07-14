#!/bin/bash
set -e

# Test script for blixard microVM creation and SSH connectivity
# This script tests the complete end-to-end flow of creating a microVM and SSH access

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
VM_NAME="test-ssh-vm"
VM_IP="10.0.0.1"
NODE_ID=1
NODE_BIND="127.0.0.1:7001"
DATA_DIR="./test-data"
NODE_PID_FILE="$DATA_DIR/node.pid"
TIMEOUT=30

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    
    # Stop VM if running
    if cargo run -- vm list 2>/dev/null | grep -q "$VM_NAME"; then
        log_info "Stopping VM $VM_NAME"
        cargo run -- vm stop --name "$VM_NAME" 2>/dev/null || true
    fi
    
    # Stop node if running
    if [[ -f "$NODE_PID_FILE" ]]; then
        local pid=$(cat "$NODE_PID_FILE")
        if kill -0 "$pid" 2>/dev/null; then
            log_info "Stopping blixard node (PID: $pid)"
            kill "$pid" 2>/dev/null || true
            sleep 2
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$NODE_PID_FILE"
    fi
    
    # Clean up data directory
    rm -rf "$DATA_DIR"
    
    log_success "Cleanup completed"
}

# Set up trap for cleanup on exit
trap cleanup EXIT

# Function to wait for condition with timeout
wait_for_condition() {
    local condition="$1"
    local description="$2"
    local timeout=${3:-$TIMEOUT}
    local interval=1
    local elapsed=0
    
    log_info "Waiting for: $description (timeout: ${timeout}s)"
    
    while ! eval "$condition" 2>/dev/null; do
        if [[ $elapsed -ge $timeout ]]; then
            log_error "Timeout waiting for: $description"
            return 1
        fi
        sleep $interval
        elapsed=$((elapsed + interval))
        echo -n "."
    done
    echo
    log_success "$description"
    return 0
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check if user is in required groups
    if ! groups | grep -q kvm; then
        log_warning "User not in 'kvm' group. You may need to run: sudo usermod -a -G kvm \$USER"
    fi
    
    # Check if cargo/rust is available
    if ! command -v cargo &> /dev/null; then
        log_error "cargo not found. Please install Rust/Cargo"
        exit 1
    fi
    
    # Check if nix is available (for VM builds)
    if ! command -v nix &> /dev/null; then
        log_warning "nix not found. VM builds may fail"
    fi
    
    # Build the project
    log_info "Building blixard..."
    cargo build --release
    
    log_success "Prerequisites checked"
}

# Setup networking (requires sudo)
setup_networking() {
    log_info "Setting up networking..."
    
    # Check if tap interface already exists
    if ip link show vm1 &>/dev/null; then
        log_info "Tap interface vm1 already exists"
    else
        log_info "Creating tap interface vm1 (requires sudo)"
        sudo ip tuntap add dev vm1 mode tap group kvm multi_queue || {
            log_error "Failed to create tap interface. Please ensure you have sudo privileges"
            exit 1
        }
        sudo ip link set dev vm1 up
        sudo ip addr add "10.0.0.0/32" dev vm1
        sudo ip route add "10.0.0.1/32" dev vm1
    fi
    
    # Check TUN device permissions
    if [[ ! -c /dev/net/tun ]]; then
        log_error "/dev/net/tun not found. TUN/TAP support may not be available"
        exit 1
    fi
    
    log_success "Networking setup completed"
}

# Start blixard node
start_node() {
    log_info "Starting blixard node..."
    
    # Kill any existing blixard processes
    pkill -f "blixard node" 2>/dev/null || true
    sleep 2
    
    # Create data directory
    mkdir -p "$DATA_DIR"
    
    # Start node in background
    nohup cargo run --release -- node --id "$NODE_ID" --bind "$NODE_BIND" --data-dir "$DATA_DIR" \
        > "$DATA_DIR/node.log" 2>&1 &
    local pid=$!
    echo $pid > "$NODE_PID_FILE"
    
    # Wait for node to start (check for registry file instead of vm list)
    wait_for_condition \
        "[[ -f '$DATA_DIR/node-$NODE_ID-registry.json' ]]" \
        "blixard node to start (registry file)" \
        30
    
    # Wait for services to be ready (Iroh services startup)
    wait_for_condition \
        "grep -q 'Blixard node started successfully' '$DATA_DIR/node.log'" \
        "blixard services to start" \
        30
    
    # Additional wait for client connection readiness
    sleep 2
    
    # Verify node is responding (try multiple times)
    local retries=5
    local success=false
    for ((i=1; i<=retries; i++)); do
        log_info "Testing node connection (attempt $i/$retries)..."
        if timeout 10 cargo run --release -- vm list &>/dev/null; then
            success=true
            break
        fi
        sleep 2
    done
    
    if [[ "$success" != "true" ]]; then
        log_error "Node started but not responding to commands after $retries attempts. Checking logs..."
        if [[ -f "$DATA_DIR/node.log" ]]; then
            echo -e "\n${YELLOW}=== Node Startup Log ===${NC}"
            tail -30 "$DATA_DIR/node.log"
        fi
        return 1
    fi
    
    log_success "Blixard node started (PID: $pid)"
}

# Create and start VM
create_and_start_vm() {
    log_info "Creating VM: $VM_NAME"
    
    # Create VM
    if ! cargo run --release -- vm create --name "$VM_NAME"; then
        log_error "Failed to create VM"
        exit 1
    fi
    
    log_success "VM created: $VM_NAME"
    
    # Start VM
    log_info "Starting VM: $VM_NAME"
    if ! cargo run --release -- vm start --name "$VM_NAME"; then
        log_error "Failed to start VM"
        exit 1
    fi
    
    # Wait for VM to boot
    wait_for_condition \
        "ping -c 1 -W 2 $VM_IP" \
        "VM to respond to ping" \
        60
    
    log_success "VM started and responding to ping"
}

# Test SSH connectivity
test_ssh() {
    log_info "Testing SSH connectivity to VM at $VM_IP"
    
    # Wait for SSH service to be ready
    wait_for_condition \
        "nc -z $VM_IP 22" \
        "SSH service to be available" \
        60
    
    # Test SSH connection (with timeout and no host key checking)
    log_info "Attempting SSH connection..."
    if timeout 10 ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        root@"$VM_IP" "echo 'SSH connection successful' && uname -a"; then
        log_success "SSH connection test passed!"
        return 0
    else
        log_error "SSH connection test failed"
        return 1
    fi
}

# Show VM status and logs
show_status() {
    log_info "Showing current status..."
    
    echo -e "\n${BLUE}=== VM List ===${NC}"
    cargo run --release -- vm list || true
    
    echo -e "\n${BLUE}=== Network Status ===${NC}"
    ip addr show vm1 2>/dev/null || echo "vm1 interface not found"
    
    echo -e "\n${BLUE}=== VM Ping Test ===${NC}"
    ping -c 3 "$VM_IP" || echo "VM not responding to ping"
    
    echo -e "\n${BLUE}=== SSH Port Test ===${NC}"
    nc -z "$VM_IP" 22 && echo "SSH port 22 is open" || echo "SSH port 22 is not responding"
    
    if [[ -f "$DATA_DIR/node.log" ]]; then
        echo -e "\n${BLUE}=== Node Log (last 20 lines) ===${NC}"
        tail -20 "$DATA_DIR/node.log"
    fi
}

# Main test execution
main() {
    echo -e "${BLUE}================================================${NC}"
    echo -e "${BLUE}  Blixard microVM SSH Connectivity Test${NC}"
    echo -e "${BLUE}================================================${NC}"
    
    check_prerequisites
    setup_networking
    start_node
    create_and_start_vm
    
    if test_ssh; then
        echo -e "\n${GREEN}================================================${NC}"
        echo -e "${GREEN}  🎉 ALL TESTS PASSED! 🎉${NC}"
        echo -e "${GREEN}================================================${NC}"
        echo -e "${GREEN}You can now SSH into the VM with:${NC}"
        echo -e "${GREEN}  ssh root@$VM_IP${NC}"
        echo -e "${GREEN}(No password required)${NC}"
        
        # Keep VM running for manual testing
        log_info "VM is running. Press Ctrl+C to stop and cleanup."
        read -p "Press Enter to cleanup and exit..."
    else
        echo -e "\n${RED}================================================${NC}"
        echo -e "${RED}  ❌ TESTS FAILED${NC}"
        echo -e "${RED}================================================${NC}"
        show_status
        exit 1
    fi
}

# Run main function
main "$@"