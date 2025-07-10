#!/bin/bash

# Fix missing source field in BlixardError::Internal
# This handles the pattern: BlixardError::Internal { message: "...", }
# And converts to: BlixardError::Internal { message: "...", source: None, }

echo "Fixing BlixardError::Internal missing source fields..."

# Get files with this specific error
cd /home/brittonr/git/blixard
files=(
    "blixard-core/src/cluster_state.rs"
    "blixard-core/src/iroh_transport_v2.rs"
    "blixard-core/src/ip_pool.rs"
    "blixard-core/src/vm_scheduler.rs"
    "blixard-core/src/abstractions/command.rs"
    "blixard-core/src/patterns/lifecycle.rs"
    "blixard-core/src/patterns/resource_pool.rs"
    "blixard-core/src/vm_network_isolation.rs"
    "blixard-core/src/raft/state_machine.rs"
)

for file in "${files[@]}"; do
    if [ -f "$file" ]; then
        echo "Processing $file..."
        
        # Fix pattern: message: "...", } 
        sed -i 's/message: \([^,}]*\),\s*}/message: \1, source: None }/g' "$file"
        
        # Fix pattern: message: "...", })
        sed -i 's/message: \([^,}]*\),\s*})/message: \1, source: None })/g' "$file"
        
        # Fix pattern: message: "..." }
        sed -i 's/message: \([^,}]*\)\s*}/message: \1, source: None }/g' "$file"
        
        # Fix pattern: message: "..." })
        sed -i 's/message: \([^,}]*\)\s*})/message: \1, source: None })/g' "$file"
        
    else
        echo "File not found: $file"
    fi
done

echo "Done fixing Internal error source fields"