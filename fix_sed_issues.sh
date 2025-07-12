#!/bin/bash

# Fix specific sed replacement issues
cd /home/brittonr/git/blixard

echo "Fixing sed replacement issues..."

# Fix the monitoring_statestd issue
sed -i 's/monitoring_statestd::time::Duration::from_secs(0)/monitoring_state.cluster_metrics.last_updated.elapsed()/g' blixard/src/tui/ui.rs

# Fix monitoring_state0.0 and monitoring_state0 issues
sed -i 's/app\.monitoring_state0\.0/0.0/g' blixard/src/tui/ui.rs
sed -i 's/app\.monitoring_state0/0/g' blixard/src/tui/ui.rs

# Fix the search_mode dereference issue
sed -i 's/\*app\.ui_state\.search_mode/app.ui_state.search_mode/g' blixard/src/tui/ui.rs

# Fix any remaining .recent_events that weren't caught
sed -i 's/^[[:space:]]*\.recent_events/.monitoring_state.recent_events/g' blixard/src/tui/ui.rs

# Fix any remaining .nodes that weren't caught
sed -i 's/^[[:space:]]*\.nodes/.node_state.nodes/g' blixard/src/tui/ui.rs

# Fix latency_ms that was missed
sed -i 's/conn\.latency_ms/None/g' blixard/src/tui/ui.rs

echo "Sed issues fixed!"