#!/bin/bash

# Fix corrupted replacements
cd /home/brittonr/git/blixard

echo "Fixing corrupted replacements..."

# Fix ui_stateNone -> ui_state
sed -i 's/app\.ui_stateNone/None/g' blixard/src/tui/ui.rs

# Fix ui_state0 -> 0
sed -i 's/app\.ui_state0/0/g' blixard/src/tui/ui.rs

# Fix log_stream_config references
sed -i 's/app\.log_stream_config/app.monitoring_state.log_stream_config/g' blixard/src/tui/ui.rs

# Remove the type annotation issue by providing a concrete type
sed -i 's/if let Some(latency) = None {/if let Some(latency) = None::<u32> {/g' blixard/src/tui/ui.rs

echo "Corrupted replacements fixed!"