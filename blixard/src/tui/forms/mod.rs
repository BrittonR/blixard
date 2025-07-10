//! Form definitions for the TUI

pub mod vm;
pub mod node;
pub mod cluster;
pub mod export_import;
pub mod settings;

// Re-export commonly used forms
pub use vm::{CreateVmForm, CreateVmField, VmMigrationForm, VmMigrationField};
pub use node::{CreateNodeForm, CreateNodeField, EditNodeForm, EditNodeField};
pub use cluster::{CreateClusterForm, CreateClusterField};
pub use export_import::{ExportForm, ImportForm};
pub use settings::{SettingsForm, SaveConfigField};

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    pub title: String,
    pub message: String,
    pub confirm_text: String,
    pub cancel_text: String,
    pub is_dangerous: bool,
}