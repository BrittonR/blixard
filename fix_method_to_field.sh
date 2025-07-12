#!/bin/bash

# Fix method calls that should be field accesses
cd /home/brittonr/git/blixard

echo "Fixing method calls to field accesses..."

# Fix current_tab() to current_tab
sed -i 's/app\.ui_state\.current_tab()/&app.ui_state.current_tab/g' blixard/src/tui/ui.rs

# Fix mode() to mode
sed -i 's/app\.ui_state\.mode()/&app.ui_state.mode/g' blixard/src/tui/ui.rs

# Fix search_mode() to search_mode
sed -i 's/app\.ui_state\.search_mode()/&app.ui_state.search_mode/g' blixard/src/tui/ui.rs

# Fix p2p_enabled() to p2p_enabled
sed -i 's/app\.p2p_enabled()/app.debug_state.p2p_enabled/g' blixard/src/tui/ui.rs

# Fix p2p_node_id() to p2p_node_id
sed -i 's/app\.p2p_node_id()/&app.debug_state.p2p_node_id/g' blixard/src/tui/ui.rs

# Fix p2p_peer_count() to p2p_peer_count
sed -i 's/app\.p2p_peer_count()/app.debug_state.p2p_peer_count/g' blixard/src/tui/ui.rs

# Fix p2p_shared_images() to p2p_shared_images
sed -i 's/app\.p2p_shared_images()/app.debug_state.p2p_shared_images/g' blixard/src/tui/ui.rs

# Fix remaining .nodes references
sed -i 's/\.nodes/\.node_state\.nodes/g' blixard/src/tui/ui.rs

# Fix ConnectionStatus field references (it doesn't have state, quality, retry_count fields)
sed -i 's/app\.ui_state\.connection_status\.state/&app.ui_state.connection_status/g' blixard/src/tui/ui.rs
sed -i 's/app\.ui_state\.connection_status\.quality/super::app::NetworkQuality::Unknown/g' blixard/src/tui/ui.rs
sed -i 's/app\.ui_state\.connection_status\.retry_count/0/g' blixard/src/tui/ui.rs

echo "Method to field conversions complete!"