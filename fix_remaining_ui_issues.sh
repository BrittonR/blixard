#!/bin/bash

# Fix remaining field access patterns in the TUI UI file
cd /home/brittonr/git/blixard

echo "Fixing remaining UI field access patterns..."

# Fix recent_events access
sed -i 's/app\.recent_events/app.monitoring_state.recent_events/g' blixard/src/tui/ui.rs

# Fix nodes access (should be from node_state)
sed -i 's/app\.nodes/app.node_state.nodes/g' blixard/src/tui/ui.rs

# Fix cluster_health access (should be from monitoring_state)
sed -i 's/app\.cluster_health/app.monitoring_state.cluster_health/g' blixard/src/tui/ui.rs

# Fix cluster_metrics access
sed -i 's/app\.cluster_metrics/app.monitoring_state.cluster_metrics/g' blixard/src/tui/ui.rs

# Fix vms access (should be from vm_state)
sed -i 's/app\.vms/app.vm_state.vms/g' blixard/src/tui/ui.rs

# Fix filtered_vms access
sed -i 's/app\.filtered_vms/app.vm_state.filtered_vms/g' blixard/src/tui/ui.rs

# Fix filtered_nodes access
sed -i 's/app\.filtered_nodes/app.node_state.filtered_nodes/g' blixard/src/tui/ui.rs

# Fix vm_list_state access
sed -i 's/app\.vm_list_state/app.vm_state.vm_list_state/g' blixard/src/tui/ui.rs

# Fix node_list_state access
sed -i 's/app\.node_list_state/app.node_state.node_list_state/g' blixard/src/tui/ui.rs

# Fix raft_debug_info access
sed -i 's/app\.raft_debug_info/app.debug_state.raft_debug_info/g' blixard/src/tui/ui.rs

# Fix debug_metrics access
sed -i 's/app\.debug_metrics/app.debug_state.debug_metrics/g' blixard/src/tui/ui.rs

# Fix resource status fields
sed -i 's/app\.resource_status/app.monitoring_state.cluster_resource_info/g' blixard/src/tui/ui.rs

# Fix connection_status access
sed -i 's/app\.connection_status/app.ui_state.connection_status/g' blixard/src/tui/ui.rs

# Fix settings access
sed -i 's/app\.settings/app.ui_state.settings/g' blixard/src/tui/ui.rs

# Fix status_message access
sed -i 's/app\.status_message/app.ui_state.status_message/g' blixard/src/tui/ui.rs

# Fix mode access
sed -i 's/app\.mode/app.ui_state.mode/g' blixard/src/tui/ui.rs

# Fix search_query access
sed -i 's/app\.search_query/app.ui_state.search_query/g' blixard/src/tui/ui.rs

# Fix search_mode access
sed -i 's/app\.search_mode/app.ui_state.search_mode/g' blixard/src/tui/ui.rs

# Fix input_mode access
sed -i 's/app\.input_mode/app.ui_state.input_mode/g' blixard/src/tui/ui.rs

# Fix auto_refresh access
sed -i 's/app\.auto_refresh/app.ui_state.auto_refresh/g' blixard/src/tui/ui.rs

# Fix current_tab access (this one needs special handling as it's a method)
sed -i 's/app\.current_tab/app.ui_state.current_tab/g' blixard/src/tui/ui.rs

echo "Remaining UI field access patterns fixed!"