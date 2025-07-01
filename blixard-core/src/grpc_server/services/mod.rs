//! gRPC service implementations
//!
//! Each service is implemented in its own module for better organization

mod task_service;
mod monitoring_service;

pub use task_service::TaskServiceImpl;
pub use monitoring_service::MonitoringServiceImpl;