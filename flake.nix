{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    naersk.url = "github:nix-community/naersk";
  };

  outputs = { nixpkgs, flake-utils, rust-overlay, naersk, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
      naersk' = pkgs.callPackage naersk {};

      in rec {
        # For `nix build` & `nix run`:
        defaultPackage = naersk'.buildPackage {
          src = ./.;
        };


        devShell = pkgs.mkShell {
          packages = with pkgs; [
            cargo-bloat
            cargo-edit
            cargo-outdated
            cargo-udeps
            cargo-watch
            rust-analyzer
            nixfmt
          ];
          nativeBuildInputs = with pkgs; [
            (rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analysis"
                "rust-std"
              ];
              targets = [ "wasm32-unknown-unknown" "wasm32-wasi" ];
            })
            nixfmt
            libiconv
            pkg-config
            libevdev
            rust-bindgen-unwrapped
            stdenv
            llvmPackages.bintools
            llvmPackages_16.lld
            gccMultiStdenv
            pkgsi686Linux.glibc
            clang
            clang
            rustup
            cargo-bloat
            cargo-edit
            cargo-outdated
            cargo-udeps
            cargo-watch
            rust-analyzer
            openssl
          ];
        };
      });
}

