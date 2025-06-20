# Shell functions for MadSim testing
# Add these to your shell profile (.bashrc, .zshrc, etc.) or source this file

# MadSim test functions (auto-set RUSTFLAGS)
mnt-all() {
    RUSTFLAGS="--cfg madsim" cargo nt-madsim "$@"
}

mnt-byzantine() {
    RUSTFLAGS="--cfg madsim" cargo nt-byzantine "$@"
}

mnt-clock-skew() {
    RUSTFLAGS="--cfg madsim" cargo nt-clock-skew "$@"
}

# Alternative: shorter names
madsim-all() {
    RUSTFLAGS="--cfg madsim" cargo nt-madsim "$@"
}

madsim-byzantine() {
    RUSTFLAGS="--cfg madsim" cargo nt-byzantine "$@"
}

madsim-clock-skew() {
    RUSTFLAGS="--cfg madsim" cargo nt-clock-skew "$@"
}