//! VM-related state management

use crate::tui::types::vm::{VmInfo, VmFilter};
use crate::tui::forms::vm::{CreateVmForm, VmMigrationForm};
use ratatui::widgets::{ListState, TableState};

#[derive(Debug)]
pub struct VmState {
    // VM data
    pub vms: Vec<VmInfo>,
    pub filtered_vms: Vec<VmInfo>,
    
    // UI state
    pub vm_list_state: ListState,
    pub vm_table_state: TableState,
    pub selected_vm: Option<String>,
    
    // Logs
    pub vm_logs: Vec<String>,
    pub vm_log_state: ListState,
    
    // Forms
    pub create_vm_form: CreateVmForm,
    pub vm_migration_form: VmMigrationForm,
    
    // Filtering
    pub vm_filter: VmFilter,
}

impl VmState {
    pub fn new() -> Self {
        let mut vm_list_state = ListState::default();
        vm_list_state.select(Some(0));
        
        let mut vm_table_state = TableState::default();
        vm_table_state.select(Some(0));

        Self {
            vms: Vec::new(),
            filtered_vms: Vec::new(),
            vm_list_state,
            vm_table_state,
            selected_vm: None,
            vm_logs: Vec::new(),
            vm_log_state: ListState::default(),
            create_vm_form: CreateVmForm::new(),
            vm_migration_form: VmMigrationForm::new(String::new(), 0),
            vm_filter: VmFilter::All,
        }
    }

    pub fn apply_filter(&mut self) {
        self.filtered_vms = match &self.vm_filter {
            VmFilter::All => self.vms.clone(),
            VmFilter::Running => self.vms.iter()
                .filter(|vm| matches!(vm.status, blixard_core::types::VmStatus::Running))
                .cloned()
                .collect(),
            VmFilter::Stopped => self.vms.iter()
                .filter(|vm| matches!(vm.status, blixard_core::types::VmStatus::Stopped))
                .cloned()
                .collect(),
            VmFilter::ByNode(node_id) => self.vms.iter()
                .filter(|vm| vm.node_id == *node_id)
                .cloned()
                .collect(),
        };
    }

    pub fn get_displayed_vms(&self) -> &[VmInfo] {
        if self.filtered_vms.is_empty() {
            &self.vms
        } else {
            &self.filtered_vms
        }
    }
}