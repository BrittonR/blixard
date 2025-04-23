#!/bin/bash
# scripts/run_cli.sh

# Configuration
COOKIE="khepri_cookie"
APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Get hostname
HOSTNAME=$(hostname)
TAILSCALE_NAME=$(tailscale status --json | jq -r '.Self.HostName' 2>/dev/null)
if [ -n "$TAILSCALE_NAME" ]; then
  HOSTNAME="$TAILSCALE_NAME.local"
fi

# Generate a unique CLI name
CLI_NAME="service_cli_$(date +%s)@$HOSTNAME"

# Run the CLI with the given arguments
erl -pa "$APP_DIR"/build/dev/erlang/*/ebin "$APP_DIR"/build/dev/erlang/*/_gleam_artefacts \
    -name "$CLI_NAME" \
    -setcookie "$COOKIE" \
    -noinput \
    -eval "service_manager:main_0(), init:stop()." \
    -extra "$@"
