//! VM-related forms for the TUI

use crate::tui::types::vm::{PlacementStrategy, VmInfo};

#[derive(Debug, Clone)]
pub struct CreateVmForm {
    pub name: String,
    pub vcpus: String,
    pub memory: String,
    pub config_path: String,
    pub placement_strategy: PlacementStrategy,
    pub node_id: Option<u64>,
    pub auto_start: bool,
    pub current_field: CreateVmField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateVmField {
    Name,
    Vcpus,
    Memory,
    ConfigPath,
    PlacementStrategy,
    NodeId,
    AutoStart,
}

impl CreateVmForm {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            vcpus: "2".to_string(),
            memory: "2048".to_string(),
            config_path: String::new(),
            placement_strategy: PlacementStrategy::MostAvailable,
            node_id: None,
            auto_start: true,
            current_field: CreateVmField::Name,
        }
    }

    pub fn new_with_smart_defaults(existing_vms: &[VmInfo]) -> Self {
        let mut form = Self::new();
        
        // Smart name generation
        let existing_names: Vec<&str> = existing_vms.iter().map(|vm| vm.name.as_str()).collect();
        let mut counter = 1;
        loop {
            let candidate_name = format!("vm-{:03}", counter);
            if !existing_names.contains(&candidate_name.as_str()) {
                form.name = candidate_name;
                break;
            }
            counter += 1;
        }
        
        form
    }
}

#[derive(Debug, Clone)]
pub struct VmMigrationForm {
    pub vm_name: String,
    pub source_node_id: u64,
    pub target_node_id: String,
    pub live_migration: bool,
    pub current_field: VmMigrationField,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmMigrationField {
    TargetNode,
    LiveMigration,
}

impl VmMigrationForm {
    pub fn new(vm_name: String, source_node_id: u64) -> Self {
        Self {
            vm_name,
            source_node_id,
            target_node_id: String::new(),
            live_migration: true,
            current_field: VmMigrationField::TargetNode,
        }
    }
}