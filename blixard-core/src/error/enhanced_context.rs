//! Enhanced error context chains with structured metadata and correlation
//!
//! This module provides enhanced error handling with structured metadata,
//! error correlation, and comprehensive context preservation.

use super::domain_errors::{ErrorMetadata, ErrorSeverity, ErrorCategory, RecoveryStrategy};
use super::types::BlixardError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};
use uuid::Uuid;

// =============================================================================
// Enhanced Error Context
// =============================================================================

/// Enhanced error context with structured metadata and correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// Unique error ID for tracking and correlation
    pub error_id: String,
    /// Timestamp when error occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Error metadata for classification and recovery
    pub metadata: ErrorMetadata,
    /// Error chain showing causation
    pub chain: Vec<ErrorLink>,
    /// Additional context data
    pub context_data: HashMap<String, serde_json::Value>,
    /// Performance context
    pub performance: Option<PerformanceContext>,
    /// Request trace information
    pub trace: Option<TraceContext>,
}

/// Link in the error chain showing causation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLink {
    /// Error message at this level
    pub message: String,
    /// Component that reported this error
    pub component: String,
    /// Operation that was being performed
    pub operation: String,
    /// Source code location (file:line)
    pub location: Option<String>,
    /// Additional context for this level
    pub context: HashMap<String, serde_json::Value>,
}

/// Performance context for error analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceContext {
    /// Duration of operation before error
    pub operation_duration: Duration,
    /// CPU usage during operation
    pub cpu_usage: Option<f64>,
    /// Memory usage during operation
    pub memory_usage: Option<u64>,
    /// Network latency if applicable
    pub network_latency: Option<Duration>,
    /// Disk I/O statistics if applicable
    pub disk_io: Option<DiskIoStats>,
}

/// Disk I/O statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIoStats {
    /// Bytes read
    pub bytes_read: u64,
    /// Bytes written
    pub bytes_written: u64,
    /// Read operations count
    pub read_ops: u64,
    /// Write operations count
    pub write_ops: u64,
}

/// Distributed tracing context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Trace ID for distributed tracing
    pub trace_id: String,
    /// Span ID for this operation
    pub span_id: String,
    /// Parent span ID if applicable
    pub parent_span_id: Option<String>,
    /// Baggage items for trace context
    pub baggage: HashMap<String, String>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(metadata: ErrorMetadata) -> Self {
        Self {
            error_id: Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
            chain: Vec::new(),
            context_data: HashMap::new(),
            performance: None,
            trace: None,
        }
    }

    /// Add a link to the error chain
    pub fn add_chain_link(mut self, component: impl Into<String>, operation: impl Into<String>, message: impl Into<String>) -> Self {
        self.chain.push(ErrorLink {
            message: message.into(),
            component: component.into(),
            operation: operation.into(),
            location: None,
            context: HashMap::new(),
        });
        self
    }

    /// Add context data
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context_data.insert(key.into(), value);
        self
    }

    /// Add correlation ID
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.metadata.correlation_id = Some(correlation_id.into());
        self
    }

    /// Add performance context
    pub fn with_performance(mut self, performance: PerformanceContext) -> Self {
        self.performance = Some(performance);
        self
    }

    /// Add trace context
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Get formatted error chain
    pub fn formatted_chain(&self) -> String {
        if self.chain.is_empty() {
            return "No error chain available".to_string();
        }

        let mut result = String::new();
        for (i, link) in self.chain.iter().enumerate() {
            result.push_str(&format!(
                "{}. {} in {} ({}): {}\n",
                i + 1,
                link.operation,
                link.component,
                link.location.as_deref().unwrap_or("unknown"),
                link.message
            ));
        }
        result
    }

    /// Get root cause from error chain
    pub fn root_cause(&self) -> Option<&ErrorLink> {
        self.chain.first()
    }

    /// Check if error is retryable based on metadata
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.metadata.recovery,
            RecoveryStrategy::Retry { .. } | RecoveryStrategy::Failover { .. }
        )
    }

    /// Get retry delay if applicable
    pub fn retry_delay(&self) -> Option<Duration> {
        match &self.metadata.recovery {
            RecoveryStrategy::Retry { base_delay, .. } => Some(*base_delay),
            _ => None,
        }
    }

    /// Get error severity
    pub fn severity(&self) -> ErrorSeverity {
        self.metadata.severity
    }

    /// Get error category
    pub fn category(&self) -> &ErrorCategory {
        &self.metadata.category
    }
}

// =============================================================================
// Enhanced Error Type
// =============================================================================

/// Enhanced error type that wraps BlixardError with additional context
#[derive(Debug, Clone)]
pub struct EnhancedError {
    /// Base error
    pub base_error: BlixardError,
    /// Enhanced context
    pub context: ErrorContext,
}

impl EnhancedError {
    /// Create a new enhanced error from BlixardError
    pub fn from_base(base_error: BlixardError, metadata: ErrorMetadata) -> Self {
        Self {
            base_error,
            context: ErrorContext::new(metadata),
        }
    }

    /// Create an enhanced error with full context
    pub fn new(base_error: BlixardError, context: ErrorContext) -> Self {
        Self {
            base_error,
            context,
        }
    }

    /// Add context to the error
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context = self.context.with_context(key, value);
        self
    }

    /// Add a chain link
    pub fn add_chain_link(mut self, component: impl Into<String>, operation: impl Into<String>, message: impl Into<String>) -> Self {
        self.context = self.context.add_chain_link(component, operation, message);
        self
    }

    /// Add performance context
    pub fn with_performance(mut self, performance: PerformanceContext) -> Self {
        self.context = self.context.with_performance(performance);
        self
    }

    /// Add trace context
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.context = self.context.with_trace(trace);
        self
    }

    /// Convert to JSON for logging/serialization
    pub fn to_json(&self) -> serde_json::Result<String> {
        let error_data = serde_json::json!({
            "error_id": self.context.error_id,
            "timestamp": self.context.timestamp,
            "base_error": self.base_error.to_string(),
            "metadata": self.context.metadata,
            "chain": self.context.chain,
            "context_data": self.context.context_data,
            "performance": self.context.performance,
            "trace": self.context.trace,
        });
        serde_json::to_string_pretty(&error_data)
    }

    /// Get structured error information for observability
    pub fn structured_info(&self) -> HashMap<String, serde_json::Value> {
        let mut info = HashMap::new();
        info.insert("error_id".to_string(), self.context.error_id.clone().into());
        info.insert("error_code".to_string(), self.context.metadata.code.clone().into());
        info.insert("severity".to_string(), format!("{:?}", self.context.metadata.severity).into());
        info.insert("category".to_string(), format!("{:?}", self.context.metadata.category).into());
        info.insert("component".to_string(), self.context.metadata.component.clone().into());
        info.insert("operation".to_string(), self.context.metadata.operation.clone().into());
        info.insert("retryable".to_string(), self.context.is_retryable().into());
        
        if let Some(correlation_id) = &self.context.metadata.correlation_id {
            info.insert("correlation_id".to_string(), correlation_id.clone().into());
        }
        
        if let Some(performance) = &self.context.performance {
            info.insert("operation_duration_ms".to_string(), performance.operation_duration.as_millis().into());
        }
        
        info
    }
}

impl fmt::Display for EnhancedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.context.error_id, self.base_error)?;
        
        if !self.context.chain.is_empty() {
            write!(f, "\nError Chain:\n{}", self.context.formatted_chain())?;
        }
        
        if let Some(correlation_id) = &self.context.metadata.correlation_id {
            write!(f, "\nCorrelation ID: {}", correlation_id)?;
        }
        
        Ok(())
    }
}

impl std::error::Error for EnhancedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.base_error)
    }
}

// =============================================================================
// Error Context Builder
// =============================================================================

/// Builder for creating enhanced error contexts
pub struct ErrorContextBuilder {
    metadata: ErrorMetadata,
    chain: Vec<ErrorLink>,
    context_data: HashMap<String, serde_json::Value>,
    performance: Option<PerformanceContext>,
    trace: Option<TraceContext>,
    start_time: Instant,
}

impl ErrorContextBuilder {
    /// Create a new error context builder
    pub fn new(metadata: ErrorMetadata) -> Self {
        Self {
            metadata,
            chain: Vec::new(),
            context_data: HashMap::new(),
            performance: None,
            trace: None,
            start_time: Instant::now(),
        }
    }

    /// Add a chain link
    pub fn add_chain_link(mut self, component: impl Into<String>, operation: impl Into<String>, message: impl Into<String>) -> Self {
        self.chain.push(ErrorLink {
            message: message.into(),
            component: component.into(),
            operation: operation.into(),
            location: None,
            context: HashMap::new(),
        });
        self
    }

    /// Add chain link with source location
    pub fn add_chain_link_with_location(
        mut self,
        component: impl Into<String>,
        operation: impl Into<String>,
        message: impl Into<String>,
        file: &str,
        line: u32,
    ) -> Self {
        self.chain.push(ErrorLink {
            message: message.into(),
            component: component.into(),
            operation: operation.into(),
            location: Some(format!("{}:{}", file, line)),
            context: HashMap::new(),
        });
        self
    }

    /// Add context data
    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context_data.insert(key.into(), value);
        self
    }

    /// Add correlation ID
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.metadata.correlation_id = Some(correlation_id.into());
        self
    }

    /// Add trace context
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Record performance metrics automatically
    pub fn with_auto_performance(mut self) -> Self {
        let duration = self.start_time.elapsed();
        self.performance = Some(PerformanceContext {
            operation_duration: duration,
            cpu_usage: None, // Could be populated with actual metrics
            memory_usage: None,
            network_latency: None,
            disk_io: None,
        });
        self
    }

    /// Build the error context
    pub fn build(self) -> ErrorContext {
        ErrorContext {
            error_id: Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            metadata: self.metadata,
            chain: self.chain,
            context_data: self.context_data,
            performance: self.performance,
            trace: self.trace,
        }
    }

    /// Build an enhanced error with the given base error
    pub fn build_enhanced_error(self, base_error: BlixardError) -> EnhancedError {
        EnhancedError::new(base_error, self.build())
    }
}

// =============================================================================
// Convenience Macros for Enhanced Error Creation
// =============================================================================

/// Macro to create an enhanced error with automatic source location
#[macro_export]
macro_rules! enhanced_error {
    ($base_error:expr, $metadata:expr) => {{
        $crate::error::enhanced_context::ErrorContextBuilder::new($metadata)
            .add_chain_link_with_location(
                $metadata.component.clone(),
                $metadata.operation.clone(),
                $base_error.to_string(),
                file!(),
                line!(),
            )
            .with_auto_performance()
            .build_enhanced_error($base_error)
    }};
    ($base_error:expr, $metadata:expr, $($key:expr => $value:expr),* $(,)?) => {{
        let mut builder = $crate::error::enhanced_context::ErrorContextBuilder::new($metadata)
            .add_chain_link_with_location(
                $metadata.component.clone(),
                $metadata.operation.clone(),
                $base_error.to_string(),
                file!(),
                line!(),
            )
            .with_auto_performance();
        $(
            builder = builder.with_context($key, serde_json::json!($value));
        )*
        builder.build_enhanced_error($base_error)
    }};
}

/// Macro to add context to an existing error result
#[macro_export]
macro_rules! with_enhanced_context {
    ($result:expr, $metadata:expr) => {
        $result.map_err(|e| {
            $crate::enhanced_error!(e, $metadata)
        })
    };
    ($result:expr, $metadata:expr, $($key:expr => $value:expr),* $(,)?) => {
        $result.map_err(|e| {
            $crate::enhanced_error!(e, $metadata, $($key => $value),*)
        })
    };
}

// =============================================================================
// Error Analysis and Reporting
// =============================================================================

/// Error analysis utilities
pub struct ErrorAnalyzer;

impl ErrorAnalyzer {
    /// Analyze error patterns for debugging
    pub fn analyze_error_pattern(errors: &[EnhancedError]) -> ErrorPattern {
        let mut component_counts = HashMap::new();
        let mut error_code_counts = HashMap::new();
        let mut severity_counts = HashMap::new();
        
        for error in errors {
            *component_counts.entry(error.context.metadata.component.clone()).or_insert(0) += 1;
            *error_code_counts.entry(error.context.metadata.code.clone()).or_insert(0) += 1;
            *severity_counts.entry(error.context.metadata.severity).or_insert(0) += 1;
        }
        
        ErrorPattern {
            total_errors: errors.len(),
            component_distribution: component_counts,
            error_code_distribution: error_code_counts,
            severity_distribution: severity_counts,
            time_range: errors.iter().map(|e| e.context.timestamp).collect(),
        }
    }

    /// Get error correlation suggestions
    pub fn suggest_correlations(error: &EnhancedError) -> Vec<String> {
        let mut suggestions = Vec::new();
        
        // Correlation based on error metadata
        if let Some(correlation_id) = &error.context.metadata.correlation_id {
            suggestions.push(format!("Search for other errors with correlation ID: {}", correlation_id));
        }
        
        // Correlation based on trace context
        if let Some(trace) = &error.context.trace {
            suggestions.push(format!("Search for errors in trace: {}", trace.trace_id));
        }
        
        // Correlation based on component
        suggestions.push(format!("Search for other errors in component: {}", error.context.metadata.component));
        
        // Correlation based on time window
        let time_window = chrono::Duration::minutes(5);
        let start_time = error.context.timestamp - time_window;
        let end_time = error.context.timestamp + time_window;
        suggestions.push(format!("Search for errors between {} and {}", start_time, end_time));
        
        suggestions
    }
}

/// Error pattern analysis result
#[derive(Debug, Clone)]
pub struct ErrorPattern {
    pub total_errors: usize,
    pub component_distribution: HashMap<String, usize>,
    pub error_code_distribution: HashMap<String, usize>,
    pub severity_distribution: HashMap<ErrorSeverity, usize>,
    pub time_range: Vec<chrono::DateTime<chrono::Utc>>,
}

impl ErrorPattern {
    /// Get the most problematic component
    pub fn most_problematic_component(&self) -> Option<&String> {
        self.component_distribution
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(component, _)| component)
    }

    /// Get the most common error code
    pub fn most_common_error_code(&self) -> Option<&String> {
        self.error_code_distribution
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(code, _)| code)
    }

    /// Check if there's an error spike
    pub fn has_error_spike(&self, threshold: usize) -> bool {
        self.total_errors > threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::domain_errors::*;

    #[test]
    fn test_error_context_creation() {
        let metadata = ErrorMetadata::new(
            "TEST_ERROR",
            ErrorSeverity::Error,
            ErrorCategory::Application,
            RecoveryStrategy::Retry {
                max_attempts: 3,
                base_delay: Duration::from_secs(1),
            },
            "test_component",
            "test_operation",
        );

        let context = ErrorContext::new(metadata)
            .add_chain_link("component1", "operation1", "first error")
            .add_chain_link("component2", "operation2", "second error")
            .with_context("key1", serde_json::json!("value1"));

        assert_eq!(context.chain.len(), 2);
        assert_eq!(context.context_data.len(), 1);
        assert!(context.is_retryable());
    }

    #[test]
    fn test_enhanced_error_creation() {
        let metadata = ErrorMetadata::new(
            "TEST_ERROR",
            ErrorSeverity::Error,
            ErrorCategory::Application,
            RecoveryStrategy::Manual {
                action_required: "Fix manually".to_string(),
            },
            "test_component",
            "test_operation",
        );

        let base_error = BlixardError::Internal {
            message: "Test error".to_string(),
        };

        let enhanced = EnhancedError::from_base(base_error, metadata)
            .with_context("test_key", serde_json::json!("test_value"))
            .add_chain_link("test_component", "test_operation", "test error");

        assert!(!enhanced.context.is_retryable());
        assert_eq!(enhanced.context.metadata.code, "TEST_ERROR");
        assert_eq!(enhanced.context.chain.len(), 1);
    }

    #[test]
    fn test_error_context_builder() {
        let metadata = ErrorMetadata::new(
            "BUILD_ERROR",
            ErrorSeverity::Warning,
            ErrorCategory::Infrastructure,
            RecoveryStrategy::Retry {
                max_attempts: 2,
                base_delay: Duration::from_millis(500),
            },
            "builder_component",
            "build_operation",
        );

        let base_error = BlixardError::Internal {
            message: "Builder test error".to_string(),
        };

        let enhanced = ErrorContextBuilder::new(metadata)
            .add_chain_link("step1", "operation1", "first step failed")
            .add_chain_link("step2", "operation2", "second step failed")
            .with_context("build_id", serde_json::json!("12345"))
            .with_auto_performance()
            .build_enhanced_error(base_error);

        assert_eq!(enhanced.context.chain.len(), 2);
        assert!(enhanced.context.performance.is_some());
        assert_eq!(enhanced.context.context_data.get("build_id").unwrap(), "12345");
    }

    #[test]
    fn test_error_analysis() {
        let metadata1 = ErrorMetadata::new(
            "ERROR_1",
            ErrorSeverity::Error,
            ErrorCategory::Application,
            RecoveryStrategy::Abort,
            "component_a",
            "operation_1",
        );

        let metadata2 = ErrorMetadata::new(
            "ERROR_2",
            ErrorSeverity::Warning,
            ErrorCategory::Infrastructure,
            RecoveryStrategy::Retry {
                max_attempts: 3,
                base_delay: Duration::from_secs(1),
            },
            "component_a",
            "operation_2",
        );

        let error1 = EnhancedError::from_base(
            BlixardError::Internal { message: "Error 1".to_string() },
            metadata1,
        );

        let error2 = EnhancedError::from_base(
            BlixardError::Internal { message: "Error 2".to_string() },
            metadata2,
        );

        let errors = vec![error1, error2];
        let pattern = ErrorAnalyzer::analyze_error_pattern(&errors);

        assert_eq!(pattern.total_errors, 2);
        assert_eq!(pattern.component_distribution.get("component_a"), Some(&2));
        assert_eq!(pattern.most_problematic_component(), Some(&"component_a".to_string()));
    }
}