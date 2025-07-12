//! Export/Import forms for the TUI

#[derive(Debug, Clone)]
pub struct ExportForm {
    pub file_path: String,
    pub output_path: String,
    pub cluster_name: String,
    pub include_vms: bool,
    pub include_nodes: bool,
    pub include_config: bool,
    pub include_images: bool,
    pub include_telemetry: bool,
    pub compress: bool,
    pub p2p_share: bool,
    pub current_field: ExportFormField,
}

impl ExportForm {
    pub fn new() -> Self {
        Self {
            file_path: "cluster-export.json".to_string(),
            output_path: "cluster-export.json".to_string(),
            cluster_name: String::new(),
            include_vms: true,
            include_nodes: true,
            include_config: true,
            include_images: false,
            include_telemetry: false,
            compress: false,
            p2p_share: false,
            current_field: ExportFormField::FilePath,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportForm {
    pub file_path: String,
    pub input_path: String,
    pub validate_only: bool,
    pub merge_existing: bool,
    pub merge: bool,
    pub p2p: bool,
    pub current_field: ImportFormField,
}

impl ImportForm {
    pub fn new() -> Self {
        Self {
            file_path: String::new(),
            input_path: String::new(),
            validate_only: false,
            merge_existing: false,
            merge: false,
            p2p: false,
            current_field: ImportFormField::FilePath,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormField {
    FilePath,
    OutputPath,
    ClusterName,
    IncludeVms,
    IncludeNodes,
    IncludeConfig,
    IncludeImages,
    IncludeTelemetry,
    Compress,
    P2pShare,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportFormField {
    FilePath,
    InputPath,
    ValidateOnly,
    MergeExisting,
    Merge,
    P2p,
}