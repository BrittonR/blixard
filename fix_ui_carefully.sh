#!/bin/bash

# Fix field access patterns in the TUI UI file
cd /home/brittonr/git/blixard

echo "Applying field access fixes carefully..."

# Fix log_entries access
sed -i 's/app\.log_entries/app.monitoring_state.log_entries/g' blixard/src/tui/ui.rs

# Fix save_config_field access
sed -i 's/app\.save_config_field/app.ui_state.save_config_field/g' blixard/src/tui/ui.rs

# Fix config_description access
sed -i 's/app\.config_description/app.ui_state.config_description/g' blixard/src/tui/ui.rs

# Fix config_file_path access
sed -i 's/app\.config_file_path/app.ui_state.config_file_path/g' blixard/src/tui/ui.rs

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

# Fix debug_log_entries access
sed -i 's/app\.debug_log_entries/app.debug_state.debug_log_entries/g' blixard/src/tui/ui.rs

# Fix vm_templates access
sed -i 's/app\.vm_templates/app.ui_state.vm_templates/g' blixard/src/tui/ui.rs

# Fix health_alerts access
sed -i 's/app\.health_alerts/app.monitoring_state.health_alerts/g' blixard/src/tui/ui.rs

# Fix batch_node_count access
sed -i 's/app\.batch_node_count/app.node_state.batch_node_count/g' blixard/src/tui/ui.rs

# Fix vm_migration_form access
sed -i 's/app\.vm_migration_form/app.vm_state.vm_migration_form/g' blixard/src/tui/ui.rs

echo "Field access patterns fixed carefully!"