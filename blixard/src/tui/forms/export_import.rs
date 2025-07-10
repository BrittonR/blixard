//! Export/Import forms for the TUI

#[derive(Debug, Clone)]
pub struct ExportForm {
    pub file_path: String,
    pub include_vms: bool,
    pub include_nodes: bool,
    pub include_config: bool,
    pub compress: bool,
}

impl ExportForm {
    pub fn new() -> Self {
        Self {
            file_path: "cluster-export.json".to_string(),
            include_vms: true,
            include_nodes: true,
            include_config: true,
            compress: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportForm {
    pub file_path: String,
    pub validate_only: bool,
    pub merge_existing: bool,
}

impl ImportForm {
    pub fn new() -> Self {
        Self {
            file_path: String::new(),
            validate_only: false,
            merge_existing: false,
        }
    }
}