pub mod error;
pub mod types;
pub mod node;
pub mod grpc_server;
pub mod storage;
pub mod vm_manager;

// Include the generated proto code
#[cfg(not(madsim))]
pub mod proto {
    tonic::include_proto!("blixard");
}

#[cfg(madsim)]
pub mod proto {
    tonic::include_proto!("sim/blixard");
}