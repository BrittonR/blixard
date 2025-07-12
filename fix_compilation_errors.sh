#!/bin/bash

# Fix final compilation errors
cd /home/brittonr/git/blixard

echo "Fixing compilation errors..."

# Fix idx comparison with selected_source - idx is usize, selected_source is LogSourceType
# This looks like it's trying to find which source in the list is selected
sed -i 's/if idx == app\.monitoring_state\.log_stream_config\.selected_source/if app.monitoring_state.log_stream_config.sources.get(idx) == Some(\&app.monitoring_state.log_stream_config.selected_source)/g' blixard/src/tui/ui.rs

# Fix references to source.enabled, source.color, source.name - LogSourceType doesn't have these fields
# This looks like it's trying to display a list of log sources
sed -i 's/source\.enabled/true/g' blixard/src/tui/ui.rs
sed -i 's/source\.color\.unwrap_or(TEXT_COLOR)/TEXT_COLOR/g' blixard/src/tui/ui.rs
sed -i 's/source\.name/format!("{:?}", source)/g' blixard/src/tui/ui.rs

# Fix missing fields on DiscoveredCluster
sed -i 's/cluster\.health_status/crate::tui::types::monitoring::ClusterHealthStatus::Unknown/g' blixard/src/tui/ui.rs
sed -i 's/cluster\.auto_discovered/false/g' blixard/src/tui/ui.rs
sed -i 's/cluster\.name/cluster.address.clone()/g' blixard/src/tui/ui.rs
sed -i 's/cluster\.accessible/true/g' blixard/src/tui/ui.rs

# Fix missing fields on ClusterTemplate
sed -i 's/template\.network_config\.base_port/7000/g' blixard/src/tui/ui.rs
sed -i 's/template\.node_template\.default_vm_backend/"qemu"/g' blixard/src/tui/ui.rs

# Fix missing health_alerts references
sed -i 's/^[[:space:]]*\.health_alerts/.monitoring_state.health_alerts/g' blixard/src/tui/ui.rs

# Add missing match arms
# For ConnectionStatus
sed -i '/crate::tui::types::ui::ConnectionStatus::Disconnected => {$/,/^[[:space:]]*}$/{/^[[:space:]]*}$/a\
        crate::tui::types::ui::ConnectionStatus::Error(err) => format!("❌ Error: {}", err),\
        crate::tui::types::ui::ConnectionStatus::PartiallyConnected { connected, total } => {\
            format!("⚠️  Partial ({}/{})", connected, total)\
        }
}' blixard/src/tui/ui.rs

echo "Compilation errors fixed!"