use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../blixard-core/proto/blixard.proto");
    
    // Tell cargo about our custom cfg flag
    println!("cargo:rustc-check-cfg=cfg(madsim)");
    
    // When building with madsim, we need to output to sim/ subdirectory
    let out_dir = std::env::var("OUT_DIR")?;
    
    // Always use the same build tool since we have madsim-tonic-build in build deps
    let sim_dir = PathBuf::from(&out_dir).join("sim");
    std::fs::create_dir_all(&sim_dir)?;
    
    // madsim-tonic-build handles both madsim and non-madsim cases
    tonic_build::configure()
        .out_dir(&sim_dir)
        .compile(&["../blixard-core/proto/blixard.proto"], &["../blixard-core/proto"])?;
    
    Ok(())
}