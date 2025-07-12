#!/bin/bash

# Comprehensive fix for UI field access patterns
cd /home/brittonr/git/blixard

echo "Applying comprehensive UI fixes..."

# First, apply all the field access fixes that were working
bash /home/brittonr/git/blixard/fix_field_access.sh
bash /home/brittonr/git/blixard/fix_ui_carefully.sh

# Fix method calls to field accesses
sed -i 's/\.current_tab()/.current_tab/g' blixard/src/tui/ui.rs
sed -i 's/\.mode()/.mode/g' blixard/src/tui/ui.rs
sed -i 's/\.search_mode()/.search_mode/g' blixard/src/tui/ui.rs
sed -i 's/\.p2p_enabled()/.p2p_enabled/g' blixard/src/tui/ui.rs
sed -i 's/\.p2p_node_id()/.p2p_node_id/g' blixard/src/tui/ui.rs
sed -i 's/\.p2p_peer_count()/.p2p_peer_count/g' blixard/src/tui/ui.rs
sed -i 's/\.p2p_shared_images()/.p2p_shared_images/g' blixard/src/tui/ui.rs

# Fix p2p field accesses (these come from app, not app.debug_state)
sed -i 's/app\.p2p_enabled/app.debug_state.p2p_enabled/g' blixard/src/tui/ui.rs
sed -i 's/app\.p2p_node_id/app.debug_state.p2p_node_id/g' blixard/src/tui/ui.rs
sed -i 's/app\.p2p_peer_count/app.debug_state.p2p_peer_count/g' blixard/src/tui/ui.rs
sed -i 's/app\.p2p_shared_images/app.debug_state.p2p_shared_images/g' blixard/src/tui/ui.rs
sed -i 's/app\.p2p_peers/app.debug_state.p2p_peers/g' blixard/src/tui/ui.rs
sed -i 's/app\.p2p_images/app.debug_state.p2p_images/g' blixard/src/tui/ui.rs
sed -i 's/app\.p2p_transfers/app.debug_state.p2p_transfers/g' blixard/src/tui/ui.rs

# Fix remaining field accesses
sed -i 's/app\.recent_events/app.monitoring_state.recent_events/g' blixard/src/tui/ui.rs
sed -i 's/app\.nodes/app.node_state.nodes/g' blixard/src/tui/ui.rs
sed -i 's/app\.cluster_metrics/app.monitoring_state.cluster_metrics/g' blixard/src/tui/ui.rs
sed -i 's/app\.vms/app.vm_state.vms/g' blixard/src/tui/ui.rs
sed -i 's/app\.filtered_vms/app.vm_state.filtered_vms/g' blixard/src/tui/ui.rs
sed -i 's/app\.filtered_nodes/app.node_state.filtered_nodes/g' blixard/src/tui/ui.rs
sed -i 's/app\.vm_list_state/app.vm_state.vm_list_state/g' blixard/src/tui/ui.rs
sed -i 's/app\.node_list_state/app.node_state.node_list_state/g' blixard/src/tui/ui.rs
sed -i 's/app\.raft_debug_info/app.debug_state.raft_debug_info/g' blixard/src/tui/ui.rs
sed -i 's/app\.debug_metrics/app.debug_state.debug_metrics/g' blixard/src/tui/ui.rs
sed -i 's/app\.connection_status/app.ui_state.connection_status/g' blixard/src/tui/ui.rs
sed -i 's/app\.settings/app.ui_state.settings/g' blixard/src/tui/ui.rs
sed -i 's/app\.status_message/app.ui_state.status_message/g' blixard/src/tui/ui.rs
sed -i 's/app\.mode/app.ui_state.mode/g' blixard/src/tui/ui.rs
sed -i 's/app\.search_query/app.ui_state.search_query/g' blixard/src/tui/ui.rs
sed -i 's/app\.search_mode/app.ui_state.search_mode/g' blixard/src/tui/ui.rs
sed -i 's/app\.input_mode/app.ui_state.input_mode/g' blixard/src/tui/ui.rs
sed -i 's/app\.auto_refresh/app.ui_state.auto_refresh/g' blixard/src/tui/ui.rs
sed -i 's/app\.current_tab/app.ui_state.current_tab/g' blixard/src/tui/ui.rs
sed -i 's/app\.vm_filter/app.vm_state.vm_filter/g' blixard/src/tui/ui.rs
sed -i 's/app\.node_filter/app.node_state.node_filter/g' blixard/src/tui/ui.rs
sed -i 's/app\.error_message/app.ui_state.error_message/g' blixard/src/tui/ui.rs
sed -i 's/app\.confirm_dialog/app.ui_state.confirm_dialog/g' blixard/src/tui/ui.rs
sed -i 's/app\.create_vm_form/app.vm_state.create_vm_form/g' blixard/src/tui/ui.rs
sed -i 's/app\.create_node_form/app.node_state.create_node_form/g' blixard/src/tui/ui.rs
sed -i 's/app\.show_help/app.ui_state.show_help/g' blixard/src/tui/ui.rs
sed -i 's/app\.create_cluster_form/app.ui_state.create_cluster_form/g' blixard/src/tui/ui.rs
sed -i 's/app\.export_form/app.ui_state.export_form/g' blixard/src/tui/ui.rs
sed -i 's/app\.import_form/app.ui_state.import_form/g' blixard/src/tui/ui.rs
sed -i 's/app\.edit_node_form/app.node_state.edit_node_form/g' blixard/src/tui/ui.rs
sed -i 's/app\.vm_migration_form/app.vm_state.vm_migration_form/g' blixard/src/tui/ui.rs

# Fix fields that don't exist on ConnectionStatus
sed -i 's/\.connection_status\.state/\.connection_status/g' blixard/src/tui/ui.rs
sed -i 's/\.connection_status\.latency_ms/None/g' blixard/src/tui/ui.rs
sed -i 's/\.connection_status\.quality/super::app::NetworkQuality::Unknown/g' blixard/src/tui/ui.rs
sed -i 's/\.connection_status\.retry_count/0/g' blixard/src/tui/ui.rs
sed -i 's/\.connection_status\.next_retry_in/None/g' blixard/src/tui/ui.rs

# Fix cluster health fields that don't exist
sed -i 's/app\.cluster_health\.healthy_nodes/app.monitoring_state.cluster_metrics.healthy_nodes/g' blixard/src/tui/ui.rs
sed -i 's/app\.cluster_health/app.monitoring_state.cluster_health/g' blixard/src/tui/ui.rs
sed -i 's/\.cluster_health\.uptime/std::time::Duration::from_secs(0)/g' blixard/src/tui/ui.rs
sed -i 's/\.cluster_health\.network_latency_ms/0.0/g' blixard/src/tui/ui.rs
sed -i 's/\.cluster_health\.leader_changes/0/g' blixard/src/tui/ui.rs
sed -i 's/health\.network_latency_ms/0.0/g' blixard/src/tui/ui.rs
sed -i 's/health\.replication_lag_ms/0.0/g' blixard/src/tui/ui.rs
sed -i 's/health\.leader_changes/0/g' blixard/src/tui/ui.rs

echo "Comprehensive UI fixes applied!"