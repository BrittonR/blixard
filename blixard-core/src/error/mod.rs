//! Error handling and recovery strategies for Blixard
//!
//! This module provides a comprehensive error handling framework designed for
//! distributed systems. It includes structured error types, context propagation,
//! recovery strategies, and observability integration.
//!
//! ## Error Handling Philosophy
//!
//! Blixard follows these principles for robust error handling:
//!
//! ### 1. Structured Error Types
//! All errors are structured with sufficient context for debugging and recovery:
//! - **Operation Context**: What operation was being performed
//! - **Resource Context**: Which resource was involved
//! - **Cause Chain**: Full error chain from root cause to user error
//! - **Actionable Information**: What the user can do to fix the issue
//!
//! ### 2. Error Recovery
//! Different error types have different recovery strategies:
//! - **Transient Errors**: Automatic retry with exponential backoff
//! - **Resource Errors**: Graceful degradation or resource cleanup
//! - **Logic Errors**: Fail fast with detailed context
//! - **System Errors**: Escalate to higher-level recovery mechanisms
//!
//! ### 3. Observability
//! All errors are instrumented for monitoring and alerting:
//! - **Structured Logging**: Consistent error logging with context
//! - **Metrics**: Error rate tracking by type and operation
//! - **Tracing**: Full request traces including error propagation
//! - **Alerting**: Critical errors trigger immediate notifications
//!
//! ## Error Categories
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Blixard Error Taxonomy                       │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  System Errors          │  Resource Errors     │  Logic Errors  │
//! │  ┌─────────────────┐    │  ┌─────────────────┐  │  ┌───────────┐ │
//! │  │ • Storage       │    │  │ • Quota Exceed  │  │  │ • Config  │ │
//! │  │ • Network       │    │  │ • Not Found     │  │  │ • Invalid │ │
//! │  │ • IO            │    │  │ • Unavailable   │  │  │ • Validate│ │
//! │  │ • Internal      │    │  │ • Exhausted     │  │  │ • Auth    │ │
//! │  └─────────────────┘    │  └─────────────────┘  │  └───────────┘ │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Distributed Errors     │  Operational Errors  │  Temporary     │
//! │  ┌─────────────────┐    │  ┌─────────────────┐  │  ┌───────────┐ │
//! │  │ • Raft          │    │  │ • Timeout       │  │  │ • Lock    │ │
//! │  │ • Cluster       │    │  │ • Not Leader    │  │  │ • Busy    │ │
//! │  │ • P2P           │    │  │ • Not Init      │  │  │ • Retry   │ │
//! │  │ • Consensus     │    │  │ • Already Exists│  │  │ • Circuit │ │
//! │  └─────────────────┘    │  └─────────────────┘  │  └───────────┘ │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

pub mod types;
pub mod constructors;
pub mod conversions;

// Re-export all public types for backward compatibility
pub use types::{BlixardError, Result, BlixardResult, format_errors};

// Import all constructor and conversion implementations
// These are automatically available when BlixardError is in scope
pub use constructors::*;
pub use conversions::*;