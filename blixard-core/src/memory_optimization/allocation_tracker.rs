//! Allocation tracking for profiling and optimization
//!
//! This module provides utilities for tracking memory allocations
//! during development to identify hot spots and optimization opportunities.

#[cfg(feature = "allocation-tracking")]
use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(feature = "allocation-tracking")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "allocation-tracking")]
use std::sync::RwLock;
#[cfg(feature = "allocation-tracking")]
use std::collections::HashMap;
#[cfg(feature = "allocation-tracking")]
use std::backtrace::Backtrace;

#[cfg(feature = "allocation-tracking")]
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "allocation-tracking")]
static ALLOCATION_BYTES: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "allocation-tracking")]
static DEALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "allocation-tracking")]
static DEALLOCATION_BYTES: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "allocation-tracking")]
lazy_static::lazy_static! {
    static ref ALLOCATION_SITES: RwLock<HashMap<String, AllocationSite>> = RwLock::new(HashMap::new());
}

#[cfg(feature = "allocation-tracking")]
#[derive(Debug, Clone)]
pub struct AllocationSite {
    pub location: String,
    pub count: usize,
    pub total_bytes: usize,
    pub backtrace: Option<String>,
}

#[cfg(feature = "allocation-tracking")]
pub struct TrackingAllocator;

#[cfg(feature = "allocation-tracking")]
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        
        if !ptr.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOCATION_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            
            // Track allocation site (expensive, only for debugging)
            if std::env::var("BLIXARD_TRACK_ALLOCATION_SITES").is_ok() {
                let backtrace = Backtrace::capture();
                let location = format!("{:?}", backtrace);
                
                let mut sites = ALLOCATION_SITES.write().unwrap();
                let site = sites.entry(location.clone()).or_insert(AllocationSite {
                    location: location.clone(),
                    count: 0,
                    total_bytes: 0,
                    backtrace: Some(location),
                });
                site.count += 1;
                site.total_bytes += layout.size();
            }
        }
        
        ptr
    }
    
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        DEALLOCATION_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        System.dealloc(ptr, layout);
    }
}

/// Allocation tracking statistics
#[derive(Debug, Clone, Copy)]
pub struct AllocationStats {
    pub allocation_count: usize,
    pub allocation_bytes: usize,
    pub deallocation_count: usize,
    pub deallocation_bytes: usize,
    pub live_allocations: usize,
    pub live_bytes: usize,
}

#[cfg(feature = "allocation-tracking")]
impl AllocationStats {
    pub fn current() -> Self {
        let alloc_count = ALLOCATION_COUNT.load(Ordering::Relaxed);
        let alloc_bytes = ALLOCATION_BYTES.load(Ordering::Relaxed);
        let dealloc_count = DEALLOCATION_COUNT.load(Ordering::Relaxed);
        let dealloc_bytes = DEALLOCATION_BYTES.load(Ordering::Relaxed);
        
        Self {
            allocation_count: alloc_count,
            allocation_bytes: alloc_bytes,
            deallocation_count: dealloc_count,
            deallocation_bytes: dealloc_bytes,
            live_allocations: alloc_count.saturating_sub(dealloc_count),
            live_bytes: alloc_bytes.saturating_sub(dealloc_bytes),
        }
    }
    
    pub fn reset() {
        ALLOCATION_COUNT.store(0, Ordering::Relaxed);
        ALLOCATION_BYTES.store(0, Ordering::Relaxed);
        DEALLOCATION_COUNT.store(0, Ordering::Relaxed);
        DEALLOCATION_BYTES.store(0, Ordering::Relaxed);
        ALLOCATION_SITES.write().unwrap().clear();
    }
}

#[cfg(not(feature = "allocation-tracking"))]
impl AllocationStats {
    pub fn current() -> Self {
        Self {
            allocation_count: 0,
            allocation_bytes: 0,
            deallocation_count: 0,
            deallocation_bytes: 0,
            live_allocations: 0,
            live_bytes: 0,
        }
    }
    
    pub fn reset() {}
}

/// Allocation tracker for profiling specific code sections
pub struct AllocationTracker {
    start_stats: AllocationStats,
    name: String,
}

impl AllocationTracker {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            start_stats: AllocationStats::current(),
            name: name.into(),
        }
    }
    
    pub fn finish(self) -> AllocationReport {
        let end_stats = AllocationStats::current();
        
        AllocationReport {
            name: self.name,
            allocations: end_stats.allocation_count - self.start_stats.allocation_count,
            bytes_allocated: end_stats.allocation_bytes - self.start_stats.allocation_bytes,
            deallocations: end_stats.deallocation_count - self.start_stats.deallocation_count,
            bytes_deallocated: end_stats.deallocation_bytes - self.start_stats.deallocation_bytes,
        }
    }
}

#[derive(Debug)]
pub struct AllocationReport {
    pub name: String,
    pub allocations: usize,
    pub bytes_allocated: usize,
    pub deallocations: usize,
    pub bytes_deallocated: usize,
}

impl AllocationReport {
    pub fn net_allocations(&self) -> isize {
        self.allocations as isize - self.deallocations as isize
    }
    
    pub fn net_bytes(&self) -> isize {
        self.bytes_allocated as isize - self.bytes_deallocated as isize
    }
    
    pub fn print(&self) {
        println!("Allocation Report: {}", self.name);
        println!("  Allocations: {}", self.allocations);
        println!("  Bytes allocated: {}", self.bytes_allocated);
        println!("  Deallocations: {}", self.deallocations);
        println!("  Bytes deallocated: {}", self.bytes_deallocated);
        println!("  Net allocations: {}", self.net_allocations());
        println!("  Net bytes: {}", self.net_bytes());
    }
}

/// Track allocations for a code block
#[macro_export]
macro_rules! track_allocations {
    ($name:expr, $block:block) => {{
        let _tracker = $crate::memory_optimization::AllocationTracker::new($name);
        let result = $block;
        let report = _tracker.finish();
        if std::env::var("BLIXARD_PRINT_ALLOCATIONS").is_ok() {
            report.print();
        }
        result
    }};
}

/// Get top allocation sites (requires allocation-tracking feature)
#[cfg(feature = "allocation-tracking")]
pub fn get_top_allocation_sites(limit: usize) -> Vec<AllocationSite> {
    let sites = ALLOCATION_SITES.read().unwrap();
    let mut sorted: Vec<_> = sites.values().cloned().collect();
    sorted.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));
    sorted.truncate(limit);
    sorted
}

/// Stub for allocation site when tracking is disabled
#[cfg(not(feature = "allocation-tracking"))]
#[derive(Debug, Clone)]
pub struct AllocationSite {
    pub location: String,
    pub count: usize,
    pub total_bytes: usize,
    pub backtrace: Option<String>,
}

#[cfg(not(feature = "allocation-tracking"))]
pub fn get_top_allocation_sites(_limit: usize) -> Vec<AllocationSite> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_allocation_tracking() {
        AllocationStats::reset();
        
        let tracker = AllocationTracker::new("test");
        
        // Allocate some memory
        let mut vecs = Vec::new();
        for i in 0..10 {
            vecs.push(vec![i; 100]);
        }
        
        let report = tracker.finish();
        
        // Should have tracked allocations
        assert!(report.allocations > 0);
        assert!(report.bytes_allocated > 0);
        
        // Clean up
        drop(vecs);
    }
    
    #[test]
    fn test_track_allocations_macro() {
        let _result = track_allocations!("macro_test", {
            let mut v = Vec::new();
            for i in 0..100 {
                v.push(i);
            }
            v.len()
        });
    }
}