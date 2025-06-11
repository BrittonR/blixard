fn main() {
    // Tell Cargo about our custom cfg
    println!("cargo::rustc-check-cfg=cfg(madsim)");
    
    // Set cfg(madsim) when building simulation tests
    if std::env::var("RUSTFLAGS").unwrap_or_default().contains("--cfg madsim") {
        println!("cargo:rustc-cfg=madsim");
    }
    
    // Rerun if environment changes
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");
}