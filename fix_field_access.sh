#!/bin/bash

# Fix field access patterns in the TUI UI file
cd /home/brittonr/git/blixard

# Fix log_entries access
sed -i 's/app\.log_entries/app.monitoring_state.log_entries/g' blixard/src/tui/ui.rs

# Fix save_config_field access
sed -i 's/app\.save_config_field/app.ui_state.save_config_field/g' blixard/src/tui/ui.rs

# Fix config_description access
sed -i 's/app\.config_description/app.ui_state.config_description/g' blixard/src/tui/ui.rs

# Fix config_file_path access
sed -i 's/app\.config_file_path/app.ui_state.config_file_path/g' blixard/src/tui/ui.rs

echo "Fixed field access patterns"