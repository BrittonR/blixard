//! OpenAPI schema definitions
//!
//! This module contains all the request/response schemas for the REST API,
//! automatically generating OpenAPI 3.x documentation and validation.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

pub mod cluster;
pub mod vm;
pub mod health;
pub mod auth;
pub mod errors;

// Re-export schemas
pub use cluster::*;
pub use vm::*;
pub use health::*;
pub use auth::*;
pub use errors::*;

/// Common pagination parameters for list endpoints
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct PaginationParams {
    /// Number of items per page (1-100)
    #[validate(range(min = 1, max = 100))]
    #[serde(default = "default_limit")]
    pub limit: u32,
    
    /// Page offset (0-based)
    #[validate(range(min = 0))]
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    20
}

/// Standard API response wrapper for paginated results
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PaginatedResponse<T> {
    /// List of items for this page
    pub items: Vec<T>,
    
    /// Total number of items available
    pub total: u64,
    
    /// Number of items in this response
    pub count: u32,
    
    /// Current page offset
    pub offset: u32,
    
    /// Items per page limit
    pub limit: u32,
}

impl<T> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, total: u64, offset: u32, limit: u32) -> Self {
        let count = items.len() as u32;
        Self {
            items,
            total,
            count,
            offset,
            limit,
        }
    }
}

/// Common metadata for API resources
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ResourceMetadata {
    /// Resource unique identifier
    pub id: String,
    
    /// Resource creation timestamp (ISO 8601)
    pub created_at: chrono::DateTime<chrono::Utc>,
    
    /// Resource last update timestamp (ISO 8601)
    pub updated_at: chrono::DateTime<chrono::Utc>,
    
    /// Optional resource tags/labels
    #[serde(default)]
    pub tags: std::collections::HashMap<String, String>,
}