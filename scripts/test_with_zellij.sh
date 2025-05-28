#!/bin/bash
# Launch Blixard test environment with Zellij

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}=== Blixard Zellij Test Environment ===${NC}"
echo

# Check if zellij is installed
if ! command -v zellij &> /dev/null; then
    echo -e "${RED}Error: zellij is not installed${NC}"
    echo "Install with: nix-env -iA nixpkgs.zellij"
    exit 1
fi

# Kill any existing beam processes
echo -e "${YELLOW}Cleaning up existing Erlang processes...${NC}"
killall beam.smp 2>/dev/null || true
sleep 1

# Ensure test service exists
if [[ ! -f "$HOME/.config/systemd/user/test-http-server.service" ]]; then
    echo -e "${YELLOW}Creating test-http-server service...${NC}"
    mkdir -p "$HOME/.config/systemd/user"
    cat > "$HOME/.config/systemd/user/test-http-server.service" << EOF
[Unit]
Description=Test HTTP Server for Blixard Testing
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/python3 -m http.server 8888 --directory /tmp
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
EOF
    systemctl --user daemon-reload
    echo -e "${GREEN}Service file created${NC}"
fi

# Launch zellij with the layout
cd "$PROJECT_DIR"
echo -e "${GREEN}Launching Zellij session...${NC}"
echo
echo -e "${YELLOW}Tips:${NC}"
echo "- Use Ctrl+p then arrow keys to switch panes"
echo "- Use Ctrl+p then h/j/k/l for vim-style navigation"
echo "- Use Ctrl+t then number to switch tabs"
echo "- Press Ctrl+q to quit Zellij"
echo
echo -e "${GREEN}Starting in 2 seconds...${NC}"
sleep 2

# Check if we're already in a Zellij session
if [ -n "$ZELLIJ" ]; then
    echo -e "${GREEN}Adding Blixard test tab to current Zellij session...${NC}"
    cd "$PROJECT_DIR"
    # Create a new tab with our layout
    zellij action new-tab --layout blixard-test-tab.kdl
else
    echo -e "${YELLOW}Not in a Zellij session. Starting a new one...${NC}"
    echo
    echo -e "${YELLOW}Once in Zellij:${NC}"
    echo "- The Blixard test tab will open automatically"
    echo "- Nodes will start in the left panes"
    echo "- Commands are available in the top-right pane"
    echo "- Monitor shows live diagnostics in bottom-right"
    echo
    cd "$PROJECT_DIR"
    exec zellij --layout blixard-test-tab.kdl
fi