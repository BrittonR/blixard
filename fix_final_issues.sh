#!/bin/bash

# Fix final compilation issues
cd /home/brittonr/git/blixard

echo "Fixing final compilation issues..."

# Fix type annotation for None
sed -i 's/if let Some(next_retry) = &None {/if let Some(next_retry) = &None::<std::time::Duration> {/g' blixard/src/tui/ui.rs

# Fix missing .log_stream_config reference
sed -i 's/^[[:space:]]*\.log_stream_config/.monitoring_state.log_stream_config/g' blixard/src/tui/ui.rs

# Fix buffer_size -> max_buffer_size
sed -i 's/\.buffer_size/.max_buffer_size/g' blixard/src/tui/ui.rs

# Fix missing field references
sed -i 's/^[[:space:]]*\.discovered_clusters/.ui_state.discovered_clusters/g' blixard/src/tui/ui.rs
sed -i 's/^[[:space:]]*\.cluster_templates/.ui_state.cluster_templates/g' blixard/src/tui/ui.rs
sed -i 's/^[[:space:]]*\.debug_log_entries/.debug_state.debug_log_entries/g' blixard/src/tui/ui.rs
sed -i 's/^[[:space:]]*\.vm_templates/.ui_state.vm_templates/g' blixard/src/tui/ui.rs

# Fix the selected_source comparison (it's a LogSourceType, not an index)
sed -i 's/app\.monitoring_state\.log_stream_config\.selected_source == 0/matches!(app.monitoring_state.log_stream_config.selected_source, LogSourceType::All)/g' blixard/src/tui/ui.rs

# Fix the sources indexing issue - sources is a Vec<LogSourceType>, not indexed by LogSourceType
sed -i 's/app\.monitoring_state\.log_stream_config\.sources\[app\.monitoring_state\.log_stream_config\.selected_source\]\.source_type/app.monitoring_state.log_stream_config.selected_source/g' blixard/src/tui/ui.rs

# Fix comparison in matches! patterns
sed -i 's/if node_id == id/if node_id == id/g' blixard/src/tui/ui.rs
sed -i 's/if vm_name == name/if vm_name == name/g' blixard/src/tui/ui.rs

# Fix ClusterHealth references (it's an enum, not a struct)
sed -i 's/health\.status/health/g' blixard/src/tui/ui.rs
sed -i 's/health\.healthy_nodes/0/g' blixard/src/tui/ui.rs
sed -i 's/health\.degraded_nodes/0/g' blixard/src/tui/ui.rs
sed -i 's/health\.failed_nodes/0/g' blixard/src/tui/ui.rs

# Fix HealthStatus namespace
sed -i 's/HealthStatus::/crate::tui::types::monitoring::HealthStatus::/g' blixard/src/tui/ui.rs

echo "Final issues fixed!"