//! Settings-related forms for the TUI

use crate::tui::types::ui::{Theme, LogLevel, PerformanceMode};

#[derive(Debug, Clone)]
pub struct AppSettings {
    pub theme: Theme,
    pub refresh_rate: u64, // seconds
    pub log_level: LogLevel,
    pub auto_connect: bool,
    pub default_vm_backend: String,
    pub show_timestamps: bool,
    pub compact_mode: bool,
    pub performance_mode: PerformanceMode,
    pub vim_mode: bool,
    pub max_history_retention: usize,
    pub update_frequency_ms: u64,
    pub debug_mode: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Default,
            refresh_rate: 5,
            log_level: LogLevel::Info,
            auto_connect: true,
            default_vm_backend: "microvm".to_string(),
            show_timestamps: true,
            compact_mode: false,
            performance_mode: PerformanceMode::Balanced,
            vim_mode: false,
            max_history_retention: 1000,
            update_frequency_ms: 1000,
            debug_mode: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsForm {
    pub current_field: usize,
    pub settings: AppSettings,
}

impl SettingsForm {
    pub fn new(current_settings: AppSettings) -> Self {
        Self {
            current_field: 0,
            settings: current_settings,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SaveConfigField {
    FilePath,
    Description,
}