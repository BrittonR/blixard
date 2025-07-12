fn main() {
    // Tell cargo to check cfg(madsim)
    println!("cargo:rustc-check-cfg=cfg(madsim)");
}
