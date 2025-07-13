//! REST API module for Blixard
//!
//! This module provides a full OpenAPI 3.x compliant REST API layer that bridges
//! to the existing Iroh P2P services. It includes:
//!
//! - Full OpenAPI 3.x specification generation
//! - Request/response validation
//! - Interactive Swagger UI documentation
//! - Authentication and authorization
//! - Comprehensive error handling
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    REST API Layer (OpenAPI)                    │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  HTTP REST Endpoints  │  OpenAPI Schemas   │  Authentication   │
//! │  ┌─────────────────┐  │  ┌───────────────┐  │  ┌─────────────┐ │
//! │  │ • VM Management │  │  │ • Auto-gen    │  │  │ • Token     │ │
//! │  │ • Cluster Ops   │  │  │ • Validation  │  │  │ • Cedar     │ │
//! │  │ • Health Check  │  │  │ • Docs        │  │  │ • Middleware│ │
//! │  └─────────────────┘  │  └───────────────┘  │  └─────────────┘ │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                      Bridge Layer                               │
//! │               (Converts REST ↔ Iroh P2P)                       │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                   Existing Iroh P2P Services                   │
//! │    (High-performance internal cluster communication)           │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

#[cfg(feature = "openapi")]
pub mod rest;

#[cfg(feature = "openapi")]
pub mod schemas;

#[cfg(feature = "openapi")]
pub mod middleware;

#[cfg(feature = "openapi")]
pub mod docs;

#[cfg(feature = "openapi")]
pub mod server;

// Re-export main API components when openapi feature is enabled
#[cfg(feature = "openapi")]
pub use docs::ApiDoc;

#[cfg(feature = "openapi")]
pub use server::RestApiServer;

#[cfg(feature = "openapi")]
pub use schemas::*;