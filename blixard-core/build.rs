fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Proto compilation is no longer needed as we're using iroh_types instead
    // println!("cargo:rerun-if-changed=proto/blixard.proto");
    
    // Tell cargo to check cfg(madsim)
    println!("cargo:rustc-check-cfg=cfg(madsim)");
    
    Ok(())
}