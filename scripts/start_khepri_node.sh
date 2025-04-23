#!/bin/bash
# scripts/start_khepri_node.sh

# Configuration
COOKIE="khepri_cookie"
APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Get hostname (for Tailscale, use the tailscale hostname)
HOSTNAME=$(hostname)
TAILSCALE_NAME=$(tailscale status --json | jq -r '.Self.HostName' 2>/dev/null)
if [ -n "$TAILSCALE_NAME" ]; then
  HOSTNAME="$TAILSCALE_NAME.local"
fi

# Determine node type from arguments
if [ "$1" == "primary" ]; then
  NODE_TYPE="primary"
  NODE_NAME="khepri_node@$HOSTNAME"
  ARGS="--init-primary"
elif [ "$1" == "secondary" ]; then
  NODE_TYPE="secondary"
  NODE_NAME="khepri_node@$HOSTNAME"
  PRIMARY_NODE="$2"
  ARGS="--init-secondary $PRIMARY_NODE"
else
  echo "Usage: $0 [primary|secondary primary_node]"
  echo "Example: $0 primary"
  echo "Example: $0 secondary khepri_node@hostname.local"
  exit 1
fi

echo "Starting $NODE_TYPE Khepri node as $NODE_NAME..."

# Start the node
erl -pa "$APP_DIR"/build/dev/erlang/*/ebin "$APP_DIR"/build/dev/erlang/*/_gleam_artefacts \
    -name "$NODE_NAME" \
    -setcookie "$COOKIE" \
    -eval "service_manager:main_0()." \
    -extra $ARGS
