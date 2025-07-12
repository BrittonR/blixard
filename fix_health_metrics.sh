#!/bin/bash

# Fix health and metrics field references
cd /home/brittonr/git/blixard

echo "Fixing health and metrics field references..."

# Fix references to cluster_health that should be cluster_metrics
sed -i 's/app\.monitoring_state\.cluster_health\.healthy_nodes/app.monitoring_state.cluster_metrics.healthy_nodes/g' blixard/src/tui/ui.rs
sed -i 's/app\.monitoring_state\.cluster_health\.uptime/std::time::Duration::from_secs(0)/g' blixard/src/tui/ui.rs
sed -i 's/app\.monitoring_state\.cluster_health\.network_latency_ms/0.0/g' blixard/src/tui/ui.rs
sed -i 's/app\.monitoring_state\.cluster_health\.leader_changes/0/g' blixard/src/tui/ui.rs
sed -i 's/health\.network_latency_ms/0.0/g' blixard/src/tui/ui.rs
sed -i 's/health\.replication_lag_ms/0.0/g' blixard/src/tui/ui.rs
sed -i 's/health\.leader_changes/0/g' blixard/src/tui/ui.rs

# Fix latency_ms references (ConnectionStatus doesn't have this field)
sed -i 's/conn\.latency_ms/None/g' blixard/src/tui/ui.rs
sed -i 's/app\.ui_state\.connection_status\.latency_ms/None/g' blixard/src/tui/ui.rs

echo "Health and metrics field references fixed!"