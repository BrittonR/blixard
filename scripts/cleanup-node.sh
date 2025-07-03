#!/bin/bash
# Node cleanup script for Blixard
# Usage: ./scripts/cleanup-node.sh

echo "🧹 Blixard Node Cleanup"
echo "======================"

# Stop any running blixard processes
echo "🛑 Stopping blixard processes..."
if pgrep -f "blixard.*node" > /dev/null; then
    pkill -f "blixard.*node"
    echo "   Killed blixard node processes"
    sleep 2
else
    echo "   No blixard processes running"
fi

# Clean up data directories
echo "🗑️  Cleaning up data directories..."
CLEANED=0

# Common data directory patterns
for dir in ./data ./node-data ./test-data ./node*-data /tmp/blixard-* /tmp/blixard-test-*; do
    if [ -d "$dir" ]; then
        echo "   Removing $dir"
        rm -rf "$dir"
        CLEANED=$((CLEANED + 1))
    fi
done

# Clean up database files
echo "🗄️  Cleaning up database files..."
for file in *.redb *.redb-lock; do
    if [ -f "$file" ]; then
        echo "   Removing $file"
        rm -f "$file"
        CLEANED=$((CLEANED + 1))
    fi
done

# Clean up VM directories
echo "💿 Cleaning up VM directories..."
for dir in ./vm-configs ./vm-data ./generated-flakes; do
    if [ -d "$dir" ]; then
        echo "   Removing $dir"
        rm -rf "$dir"
        CLEANED=$((CLEANED + 1))
    fi
done

# Clean up systemd user services for VMs
echo "🔧 Cleaning up systemd VM services..."
if systemctl --user list-units --all | grep -q "blixard-vm-"; then
    systemctl --user stop blixard-vm-*.service 2>/dev/null
    
    # Remove service files
    for service in ~/.config/systemd/user/blixard-vm-*.service; do
        if [ -f "$service" ]; then
            service_name=$(basename "$service")
            echo "   Removing $service_name"
            systemctl --user disable "$service_name" 2>/dev/null
            rm -f "$service"
            CLEANED=$((CLEANED + 1))
        fi
    done
    
    systemctl --user daemon-reload
else
    echo "   No VM services found"
fi

if [ $CLEANED -gt 0 ]; then
    echo ""
    echo "✅ Cleanup complete! Removed $CLEANED items."
else
    echo ""
    echo "✅ Already clean - nothing to remove."
fi

echo ""
echo "📝 You can now start fresh with:"
echo "   export BLIXARD_P2P_ENABLED=true"
echo "   cargo run -- node --id 1 --bind 127.0.0.1:7001 --data-dir ./data"