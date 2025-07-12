//! Form definitions for the TUI

#![allow(unused_imports)]

pub mod vm;
pub mod node;
pub mod cluster;
pub mod export_import;
pub mod settings;

// Re-export commonly used forms
pub use vm::{CreateVmForm, CreateVmField, VmMigrationForm, VmMigrationField};
pub use node::{CreateNodeForm, CreateNodeField, EditNodeForm, EditNodeField};
// pub use cluster::{CreateClusterForm, CreateClusterField}; // TODO: Re-enable when implemented
pub use export_import::{ExportForm, ImportForm, ExportFormField, ImportFormField};
pub use settings::SaveConfigField;

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_text: String,
    pub cancel_text: String,
    pub is_dangerous: bool,
    pub selected: bool, // true for confirm, false for cancel
}