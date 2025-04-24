#!/bin/bash
# scripts/deploy.sh

# Configuration
DEPLOY_DIR="/opt/service_manager"
APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Check if run as root
if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root" 
   exit 1
fi

# Create deployment directory if it doesn't exist
mkdir -p "$DEPLOY_DIR"

# Copy app files
echo "Copying application files to $DEPLOY_DIR..."
cp -r "$APP_DIR"/build "$DEPLOY_DIR/"
cp -r "$APP_DIR"/scripts "$DEPLOY_DIR/"

# Create command symlinks
ln -sf "$DEPLOY_DIR/scripts/run_cli.sh" /usr/local/bin/service_manager
ln -sf "$DEPLOY_DIR/scripts/start_khepri_node.sh" /usr/local/bin/start_khepri_node

# Make scripts executable
chmod +x "$DEPLOY_DIR/scripts/run_cli.sh"
chmod +x "$DEPLOY_DIR/scripts/start_khepri_node.sh"

echo "Service manager deployed successfully"
echo "Usage:"
echo "  - To start the first node: start_khepri_node"
echo "  - To join an existing cluster: start_khepri_node khepri_node@hostname.local"
echo "  - To use the CLI: service_manager [command] [args]"
