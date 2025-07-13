//! Memory optimization utilities and patterns
//!
//! This module provides memory-efficient patterns and utilities to reduce allocations
//! throughout the codebase, including:
//! - Object pooling for frequently allocated types
//! - String interning and optimization
//! - Efficient collection management
//! - Arc/Rc optimization patterns

pub mod string_pool;
pub mod object_pool;
pub mod collection_utils;
pub mod arc_optimizer;
pub mod allocation_tracker;

pub use string_pool::{StringPool, InternedString};
pub use object_pool::{TypedObjectPool, PooledObject};
pub use collection_utils::{SmallVec, FlatMap, preallocate_capacity};
pub use arc_optimizer::{ArcCache, WeakCache};

#[cfg(feature = "allocation-tracking")]
pub use allocation_tracker::{AllocationTracker, track_allocations};