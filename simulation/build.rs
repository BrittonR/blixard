fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../blixard-core/proto/blixard.proto");
    println!("cargo:rerun-if-changed=proto/simulation.proto");
    
    // Tell cargo about our custom cfg flag
    println!("cargo:rustc-check-cfg=cfg(madsim)");
    
    // Create proto directory if it doesn't exist
    std::fs::create_dir_all("proto")?;
    
    // madsim-tonic-build for madsim compatibility
    // Compile only the simulation proto which includes all needed definitions
    tonic_build::compile_protos("proto/simulation.proto")?;
    
    Ok(())
}