{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

      in {
        devShell = pkgs.mkShell {

          packages = with pkgs; [
            cargo-bloat
            cargo-edit
            cargo-outdated
            cargo-udeps
            cargo-watch
            rust-analyzer
          ];
          nativeBuildInputs = with pkgs; [
            (rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analysis"
                "rust-std"
              ]; # Adjust extensions as needed
              targets = [ "wasm32-unknown-unknown" "wasm32-wasi" ];
            })
            libiconv
            pkg-config
            libevdev
            wasmtime
            wasm-pack
            wasm-bindgen-cli
            rust-bindgen-unwrapped
            cargo-wasi
            stdenv
            llvmPackages.bintools
            llvmPackages_16.lld
            gccMultiStdenv
            pkgsi686Linux.glibc
            clang
            # zstd
            dioxus-cli
            lunatic
            clang
            rustup
            cargo-bloat
            cargo-edit
            cargo-outdated
            cargo-udeps
            cargo-watch
            rust-analyzer
            tailwindcss
            nodePackages.tailwindcss
            nodejs_22
            openssl
            # Additional tools for debugging could be added here
          ];

        };
      });
}
