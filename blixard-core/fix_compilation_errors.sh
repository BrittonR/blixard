#!/bin/bash
# Quick fixes for compilation errors to get a minimal build working

echo "Applying minimal fixes to get Blixard compiling..."

# Fix NotFound error usage - replace entity/id with resource
echo "Fixing NotFound error usage..."
find src -name "*.rs" -type f -exec sed -i 's/NotFound { entity: \(.*\), id: \(.*\) }/NotFound { resource: format!("{} {}", \1, \2) }/g' {} \;
find src -name "*.rs" -type f -exec sed -i 's/NotFound { entity, id }/NotFound { resource: format!("{} {}", entity, id) }/g' {} \;

# Fix ConfigurationError - replace with ConfigError  
echo "Fixing ConfigurationError usage..."
find src -name "*.rs" -type f -exec sed -i 's/BlixardError::ConfigurationError/BlixardError::ConfigError/g' {} \;

# Add missing trait imports
echo "Adding missing Storage trait imports..."
for file in src/backup_manager.rs src/backup_replication.rs; do
    if [ -f "$file" ] && ! grep -q "use raft::Storage;" "$file"; then
        sed -i '1a use raft::Storage;' "$file"
    fi
done

# Comment out broken modules temporarily
echo "Disabling broken modules temporarily..."

# Disable backup_manager in lib.rs
sed -i 's/^pub mod backup_manager;/\/\/ pub mod backup_manager; \/\/ Temporarily disabled for compilation/g' src/lib.rs
sed -i 's/^pub mod backup_replication;/\/\/ pub mod backup_replication; \/\/ Temporarily disabled for compilation/g' src/lib.rs
sed -i 's/^pub mod audit_log;/\/\/ pub mod audit_log; \/\/ Temporarily disabled for compilation/g' src/lib.rs
sed -i 's/^pub mod audit_integration;/\/\/ pub mod audit_integration; \/\/ Temporarily disabled for compilation/g' src/lib.rs
sed -i 's/^pub mod config_hot_reload;/\/\/ pub mod config_hot_reload; \/\/ Temporarily disabled for compilation/g' src/lib.rs
sed -i 's/^pub mod remediation_engine;/\/\/ pub mod remediation_engine; \/\/ Temporarily disabled for compilation/g' src/lib.rs
sed -i 's/^pub mod remediation_raft;/\/\/ pub mod remediation_raft; \/\/ Temporarily disabled for compilation/g' src/lib.rs

echo "Fixes applied. Try building again with: cargo build -p blixard"