fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/blixard.proto");
    
    // Get the OUT_DIR environment variable
    let out_dir = std::env::var("OUT_DIR")?;
    
    // When building with madsim, madsim-tonic expects files in a sim/ subdirectory
    if std::env::var("CARGO_CFG_MADSIM").is_ok() {
        // Create sim directory
        let sim_dir = format!("{}/sim", out_dir);
        std::fs::create_dir_all(&sim_dir)?;
        
        // Configure tonic_build to output to sim directory
        tonic_build::configure()
            .out_dir(&sim_dir)
            .compile_protos(&["proto/blixard.proto"], &["proto"])?;
    } else {
        // Normal build
        tonic_build::compile_protos("proto/blixard.proto")?;
    }
    
    // Tell cargo to check cfg(madsim)
    println!("cargo:rustc-check-cfg=cfg(madsim)");
    
    Ok(())
}
