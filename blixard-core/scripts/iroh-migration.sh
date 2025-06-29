#!/bin/bash
# Iroh Transport Migration Script
# 
# This script helps manage the migration from gRPC to Iroh transport
# Usage: ./iroh-migration.sh [command] [options]

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PROMETHEUS_URL="${PROMETHEUS_URL:-http://localhost:9090}"
BLIXARD_CLI="${BLIXARD_CLI:-blixard-cli}"
CONFIG_FILE="${CONFIG_FILE:-/etc/blixard/config.toml}"
LOG_FILE="${LOG_FILE:-/var/log/blixard/migration.log}"

# Logging function
log() {
    local level=$1
    shift
    echo -e "[$(date +'%Y-%m-%d %H:%M:%S')] [${level}] $*" | tee -a "$LOG_FILE"
}

# Check prerequisites
check_prerequisites() {
    log "INFO" "Checking prerequisites..."
    
    # Check if blixard-cli is available
    if ! command -v "$BLIXARD_CLI" &> /dev/null; then
        log "ERROR" "blixard-cli not found. Please install or set BLIXARD_CLI environment variable."
        exit 1
    fi
    
    # Check if curl is available for metrics
    if ! command -v curl &> /dev/null; then
        log "ERROR" "curl not found. Please install curl."
        exit 1
    fi
    
    # Check if config file exists
    if [[ ! -f "$CONFIG_FILE" ]]; then
        log "ERROR" "Config file not found: $CONFIG_FILE"
        exit 1
    fi
    
    log "INFO" "Prerequisites check passed ✓"
}

# Get current transport mode
get_transport_mode() {
    grep -E "^mode\s*=" "$CONFIG_FILE" | awk -F'"' '{print $2}'
}

# Get migration status from metrics
get_migration_metrics() {
    local metric=$1
    curl -s "${PROMETHEUS_URL}/api/v1/query?query=${metric}" | \
        jq -r '.data.result[0].value[1] // "0"' 2>/dev/null || echo "0"
}

# Health check function
health_check() {
    log "INFO" "Performing health check..."
    
    # Check cluster status
    if ! $BLIXARD_CLI cluster status &> /dev/null; then
        log "ERROR" "Cluster status check failed"
        return 1
    fi
    
    # Check error rates
    local error_rate=$(get_migration_metrics "rate(blixard_errors_total[5m])")
    if (( $(echo "$error_rate > 0.01" | bc -l) )); then
        log "WARN" "High error rate detected: ${error_rate}"
        return 1
    fi
    
    log "INFO" "Health check passed ✓"
    return 0
}

# Enable dual mode
enable_dual_mode() {
    log "INFO" "Enabling dual transport mode..."
    
    # Backup current config
    cp "$CONFIG_FILE" "${CONFIG_FILE}.backup.$(date +%Y%m%d%H%M%S)"
    
    # Update config to dual mode
    sed -i 's/^mode\s*=.*/mode = "dual"/' "$CONFIG_FILE"
    
    # Add migration strategy if not present
    if ! grep -q "\[transport.migration\]" "$CONFIG_FILE"; then
        cat >> "$CONFIG_FILE" << EOF

[transport.migration]
prefer_iroh = ["health", "status"]
raft_transport = "grpc"
fallback_to_grpc = true
EOF
    fi
    
    log "INFO" "Dual mode enabled. Restart nodes to apply changes."
}

# Migrate specific services to Iroh
migrate_service() {
    local service=$1
    log "INFO" "Migrating service '${service}' to Iroh transport..."
    
    # Update prefer_iroh list
    local current_services=$(grep "prefer_iroh" "$CONFIG_FILE" | sed 's/.*\[\(.*\)\].*/\1/' | tr -d ' "')
    
    if [[ ! "$current_services" =~ "$service" ]]; then
        # Add service to prefer_iroh
        sed -i "/prefer_iroh/s/\]/, \"$service\"\]/" "$CONFIG_FILE"
        log "INFO" "Added '${service}' to Iroh preference list"
    else
        log "WARN" "Service '${service}' already in Iroh preference list"
    fi
}

# Monitor migration progress
monitor_migration() {
    log "INFO" "Monitoring migration progress..."
    
    while true; do
        clear
        echo -e "${BLUE}=== Iroh Migration Status ===${NC}"
        echo ""
        
        # Current mode
        local mode=$(get_transport_mode)
        echo -e "Transport Mode: ${GREEN}${mode}${NC}"
        echo ""
        
        # Connection stats
        echo "Connections:"
        local grpc_conns=$(get_migration_metrics "blixard_connections_active{transport=\"grpc\"}")
        local iroh_conns=$(get_migration_metrics "blixard_connections_active{transport=\"iroh\"}")
        echo -e "  gRPC:  ${grpc_conns}"
        echo -e "  Iroh:  ${GREEN}${iroh_conns}${NC}"
        echo ""
        
        # Message rates
        echo "Message Rate (msg/s):"
        local grpc_rate=$(get_migration_metrics "rate(blixard_messages_total{transport=\"grpc\"}[1m])")
        local iroh_rate=$(get_migration_metrics "rate(blixard_messages_total{transport=\"iroh\"}[1m])")
        printf "  gRPC:  %.2f\n" "$grpc_rate"
        printf "  Iroh:  ${GREEN}%.2f${NC}\n" "$iroh_rate"
        echo ""
        
        # Latency
        echo "P99 Latency:"
        local grpc_latency=$(get_migration_metrics "histogram_quantile(0.99,rate(blixard_message_duration_seconds_bucket{transport=\"grpc\"}[5m]))")
        local iroh_latency=$(get_migration_metrics "histogram_quantile(0.99,rate(blixard_message_duration_seconds_bucket{transport=\"iroh\"}[5m]))")
        printf "  gRPC:  %.3fs\n" "$grpc_latency"
        printf "  Iroh:  ${GREEN}%.3fs${NC}\n" "$iroh_latency"
        echo ""
        
        # NAT traversal
        local nat_success=$(get_migration_metrics "iroh:nat_traversal:success_rate")
        printf "NAT Traversal Success: ${GREEN}%.1f%%${NC}\n" "$(echo "$nat_success * 100" | bc -l)"
        echo ""
        
        # Error rates
        echo "Error Rates:"
        local grpc_errors=$(get_migration_metrics "rate(blixard_errors_total{transport=\"grpc\"}[5m])")
        local iroh_errors=$(get_migration_metrics "rate(blixard_errors_total{transport=\"iroh\"}[5m])")
        printf "  gRPC:  %.4f\n" "$grpc_errors"
        printf "  Iroh:  ${GREEN}%.4f${NC}\n" "$iroh_errors"
        
        echo ""
        echo "Press Ctrl+C to exit"
        sleep 5
    done
}

# Rollback to gRPC
rollback() {
    log "WARN" "Initiating rollback to gRPC transport..."
    
    # Update config
    sed -i 's/^mode\s*=.*/mode = "grpc"/' "$CONFIG_FILE"
    
    log "INFO" "Rollback complete. Restart nodes to apply changes."
    
    # Send alert
    if command -v mail &> /dev/null; then
        echo "Iroh transport rollback initiated at $(date)" | \
            mail -s "ALERT: Blixard Transport Rollback" oncall@company.com
    fi
}

# Validate Iroh connectivity
validate_iroh() {
    log "INFO" "Validating Iroh connectivity..."
    
    # Get list of nodes
    local nodes=$($BLIXARD_CLI cluster nodes --format json | jq -r '.[].id')
    
    for node in $nodes; do
        log "INFO" "Testing connectivity to node ${node}..."
        
        if $BLIXARD_CLI debug ping-peer --node-id "$node" --transport iroh; then
            log "INFO" "  ✓ Node ${node} reachable via Iroh"
        else
            log "ERROR" "  ✗ Node ${node} not reachable via Iroh"
            return 1
        fi
    done
    
    log "INFO" "Iroh connectivity validation passed ✓"
    return 0
}

# Main command handler
case "${1:-help}" in
    check)
        check_prerequisites
        health_check
        ;;
    
    enable-dual)
        check_prerequisites
        health_check || { log "ERROR" "Health check failed. Fix issues before proceeding."; exit 1; }
        enable_dual_mode
        ;;
    
    migrate-service)
        if [[ -z "${2:-}" ]]; then
            log "ERROR" "Service name required. Usage: $0 migrate-service <service>"
            exit 1
        fi
        check_prerequisites
        migrate_service "$2"
        ;;
    
    monitor)
        check_prerequisites
        monitor_migration
        ;;
    
    validate)
        check_prerequisites
        validate_iroh
        ;;
    
    rollback)
        check_prerequisites
        rollback
        ;;
    
    status)
        check_prerequisites
        mode=$(get_transport_mode)
        echo -e "Current transport mode: ${GREEN}${mode}${NC}"
        
        if [[ "$mode" == "dual" ]]; then
            services=$(grep "prefer_iroh" "$CONFIG_FILE" | sed 's/.*\[\(.*\)\].*/\1/' | tr -d ' "')
            echo -e "Services on Iroh: ${GREEN}${services}${NC}"
        fi
        ;;
    
    help|*)
        cat << EOF
Blixard Iroh Transport Migration Script

Usage: $0 [command] [options]

Commands:
  check           - Run prerequisite and health checks
  enable-dual     - Enable dual transport mode
  migrate-service - Migrate specific service to Iroh
  monitor         - Monitor migration progress in real-time
  validate        - Validate Iroh connectivity between nodes
  rollback        - Emergency rollback to gRPC
  status          - Show current migration status
  help            - Show this help message

Environment Variables:
  PROMETHEUS_URL  - Prometheus server URL (default: http://localhost:9090)
  BLIXARD_CLI     - Path to blixard-cli (default: blixard-cli)
  CONFIG_FILE     - Path to config file (default: /etc/blixard/config.toml)
  LOG_FILE        - Path to log file (default: /var/log/blixard/migration.log)

Examples:
  # Check prerequisites and health
  $0 check
  
  # Enable dual mode (step 1)
  $0 enable-dual
  
  # Migrate health service to Iroh
  $0 migrate-service health
  
  # Monitor migration progress
  $0 monitor
  
  # Emergency rollback
  $0 rollback

EOF
        ;;
esac