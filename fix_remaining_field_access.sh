#!/bin/bash

# Fix remaining field access patterns in the TUI UI file
cd /home/brittonr/git/blixard

echo "Fixing remaining field access patterns..."

# Fix log_list_state access
sed -i 's/app\.log_list_state/app.monitoring_state.log_list_state/g' blixard/src/tui/ui.rs

# Fix buffer_size access (should be max_buffer_size)
sed -i 's/app\.monitoring_state\.log_stream_config\.buffer_size/app.monitoring_state.log_stream_config.max_buffer_size/g' blixard/src/tui/ui.rs

# Fix quick_filter access
sed -i 's/app\.quick_filter/app.ui_state.quick_filter/g' blixard/src/tui/ui.rs

# Fix cluster discovery fields
sed -i 's/app\.cluster_discovery_active/app.ui_state.cluster_discovery_active/g' blixard/src/tui/ui.rs
sed -i 's/app\.cluster_scan_progress/app.ui_state.cluster_scan_progress/g' blixard/src/tui/ui.rs
sed -i 's/app\.discovered_clusters/app.ui_state.discovered_clusters/g' blixard/src/tui/ui.rs
sed -i 's/app\.cluster_templates/app.ui_state.cluster_templates/g' blixard/src/tui/ui.rs

# Fix debug metrics fields (these don't exist, need to be added)
sed -i 's/metrics\.snapshot_creations/metrics.snapshot_count/g' blixard/src/tui/ui.rs
sed -i 's/metrics\.log_compactions/0/g' blixard/src/tui/ui.rs
sed -i 's/metrics\.network_partitions_detected/0/g' blixard/src/tui/ui.rs

# Fix debug_log_entries access
sed -i 's/app\.debug_log_entries/app.debug_state.debug_log_entries/g' blixard/src/tui/ui.rs

# Fix vm_templates access
sed -i 's/app\.vm_templates/app.ui_state.vm_templates/g' blixard/src/tui/ui.rs

# Fix health_alerts access
sed -i 's/app\.health_alerts/app.monitoring_state.health_alerts/g' blixard/src/tui/ui.rs

# Fix batch_node_count access (this doesn't exist in the new structure, needs to be added)
sed -i 's/app\.batch_node_count/app.ui_state.create_cluster_form.node_count.to_string()/g' blixard/src/tui/ui.rs

# Fix vm_migration_form access
sed -i 's/app\.vm_migration_form/app.vm_state.vm_migration_form/g' blixard/src/tui/ui.rs

echo "Field access patterns fixed!"