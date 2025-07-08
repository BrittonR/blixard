fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tell cargo to check cfg(madsim)
    println!("cargo:rustc-check-cfg=cfg(madsim)");

    Ok(())
}
