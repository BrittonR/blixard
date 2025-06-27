//! gRPC service implementations
//!
//! Each service is implemented in its own module for better organization

mod vm_service;
mod cluster_service;
mod task_service;
mod monitoring_service;

pub use vm_service::VmServiceImpl;
pub use cluster_service::ClusterServiceImpl;
pub use task_service::TaskServiceImpl;
pub use monitoring_service::MonitoringServiceImpl;