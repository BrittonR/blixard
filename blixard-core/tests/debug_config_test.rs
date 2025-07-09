#![cfg(feature = "test-helpers")]

use blixard_core::{
    config_v2::ConfigBuilder,
    error::BlixardResult,
};

#[test]
fn test_config_builder() -> BlixardResult<()> {
    let test_config = ConfigBuilder::new()
        .node_id(1)
        .bind_address("127.0.0.1:7001")
        .data_dir("/tmp/test-blixard")
        .vm_backend("mock")
        .build()?;
    
    println!("Config built successfully: {:?}", test_config.node.id);
    Ok(())
}