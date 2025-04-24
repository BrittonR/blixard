#!/bin/bash
# scripts/start_khepri_node.sh (unified version)

# Configuration
COOKIE="khepri_cookie"
APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Get hostname (for Tailscale, use the tailscale hostname)
HOSTNAME=$(hostname)
TAILSCALE_NAME=$(tailscale status --json | jq -r '.Self.HostName' 2>/dev/null)
if [ -n "$TAILSCALE_NAME" ]; then
  HOSTNAME="$TAILSCALE_NAME.local"
fi

# Node name
NODE_NAME="khepri_node@$HOSTNAME"

# Handle connecting to existing cluster if specified
if [ -n "$1" ]; then
  SEED_NODE="$1"
  ARGS="--join-cluster $SEED_NODE"
  echo "Starting node as $NODE_NAME and joining cluster via $SEED_NODE..."
else
  ARGS="--join-cluster"
  echo "Starting node as $NODE_NAME with auto-discovery..."
fi

# Start the node
erl -pa "$APP_DIR"/build/dev/erlang/*/ebin "$APP_DIR"/build/dev/erlang/*/_gleam_artefacts \
    -name "$NODE_NAME" \
    -setcookie "$COOKIE" \
    -eval "service_manager:main_0()." \
    -extra $ARGS
