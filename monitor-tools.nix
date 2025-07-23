# Monitoring tools for Blixard
# Usage: nix-shell monitor-tools.nix

{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    curl
    jq
    
    # Quick health check script
    (writeShellScriptBin "blixard-health" ''
      #!/usr/bin/env bash
      set -euo pipefail
      
      NODE_ADDR=''${1:-127.0.0.1:7001}
      METRICS_PORT=''${2:-9090}
      
      # Replace port in address
      METRICS_ADDR="''${NODE_ADDR%:*}:$METRICS_PORT"
      
      # Quick health check
      echo -n "Checking $NODE_ADDR... "
      
      if curl -sf -m 2 "http://$METRICS_ADDR/metrics" > /dev/null 2>&1; then
        echo "✅ HEALTHY"
        
        # Get key metrics in one line
        METRICS=$(curl -sf -m 2 "http://$METRICS_ADDR/metrics" 2>/dev/null || echo "")
        
        if [ -n "$METRICS" ]; then
          IS_LEADER=$(echo "$METRICS" | grep -E "^blixard_raft_is_leader " | awk '{print $2}' || echo "0")
          TERM=$(echo "$METRICS" | grep -E "^blixard_raft_term " | awk '{print int($2)}' || echo "0")
          VMS=$(echo "$METRICS" | grep -E "^blixard_vm_total_count " | awk '{print int($2)}' || echo "0")
          RUNNING=$(echo "$METRICS" | grep -E "^blixard_vm_running_count " | awk '{print int($2)}' || echo "0")
          CPU=$(echo "$METRICS" | grep -E "^blixard_node_cpu_usage_percent " | awk '{printf "%.1f", $2}' || echo "0.0")
          MEM=$(echo "$METRICS" | grep -E "^blixard_node_memory_usage_percent " | awk '{printf "%.1f", $2}' || echo "0.0")
          PEERS=$(echo "$METRICS" | grep -E "^blixard_peer_active_connections " | awk '{print int($2)}' || echo "0")
          
          # Format output
          printf "Leader:%s Term:%s VMs:%s/%s CPU:%s%% Mem:%s%% Peers:%s\n" \
            "$([ "''${IS_LEADER%.*}" = "1" ] && echo "Y" || echo "N")" \
            "$TERM" \
            "$RUNNING" \
            "$VMS" \
            "$CPU" \
            "$MEM" \
            "$PEERS"
        else
          echo "(no metrics available)"
        fi
      else
        echo "❌ UNREACHABLE"
        exit 1
      fi
    '')
    
    # Test cluster with multiple nodes
    (writeShellScriptBin "blixard-test-cluster" ''
      #!/usr/bin/env bash
      echo "Testing 3-node cluster setup..."
      echo "=============================="
      
      for i in 1 2 3; do
        PORT=$((7000 + i))
        echo -n "Node $i (127.0.0.1:$PORT): "
        blixard-health "127.0.0.1:$PORT" || true
      done
    '')
    
    # Parse metrics robustly
    (writeShellScriptBin "blixard-metrics-json" ''
      #!/usr/bin/env bash
      set -euo pipefail
      
      NODE_ADDR=''${1:-127.0.0.1:7001}
      METRICS_PORT=''${2:-9090}
      METRICS_ADDR="''${NODE_ADDR%:*}:$METRICS_PORT"
      
      # Fetch metrics with timeout
      METRICS=$(curl -sf -m 2 "http://$METRICS_ADDR/metrics" 2>/dev/null || echo "")
      
      if [ -z "$METRICS" ]; then
        echo '{"error": "Failed to fetch metrics", "node": "'$NODE_ADDR'"}'
        exit 1
      fi
      
      # Parse metrics into JSON using awk
      echo "$METRICS" | awk '
        BEGIN { 
          print "{"
          print "  \"timestamp\": \"" strftime("%Y-%m-%dT%H:%M:%SZ", systime()) "\","
          print "  \"node\": \"'$NODE_ADDR'\","
          print "  \"metrics\": {"
          first = 1
        }
        /^blixard_/ && NF == 2 {
          if (!first) print ","
          printf "    \"%s\": %s", $1, $2
          first = 0
        }
        END {
          print ""
          print "  }"
          print "}"
        }
      '
    '')
  ];
  
  shellHook = ''
    echo "🔍 Blixard Monitoring Tools"
    echo "=========================="
    echo ""
    echo "Available commands:"
    echo "  blixard-health [addr] [port]    - Quick health check"
    echo "  blixard-test-cluster            - Test 3-node cluster"
    echo "  blixard-metrics-json [addr]     - Get metrics as JSON"
    echo ""
    echo "Examples:"
    echo "  blixard-health"
    echo "  blixard-health 192.168.1.10:7001 9090"
    echo "  blixard-metrics-json | jq '.metrics.blixard_raft_is_leader'"
    echo ""
  '';
}