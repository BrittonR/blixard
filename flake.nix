{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    naersk.url = "github:nix-community/naersk";
  };

  outputs = { nixpkgs, flake-utils, rust-overlay, naersk, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-darwin" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        naersk' = pkgs.callPackage naersk {};

        # Common packages for both systems
        commonPackages = with pkgs; [
          cargo-bloat
          cargo-edit
          cargo-outdated
          cargo-udeps
          cargo-watch
          rust-analyzer
          nixfmt
        ];

        # Common native build inputs for both systems
        commonNativeBuildInputs = with pkgs; [
          (rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analysis"
              "rust-std"
            ];
            targets = [ "wasm32-unknown-unknown" "wasm32-wasi" ];
          })
          nixfmt
          pkg-config
          rustup
          openssl
        ];

      in rec {
        # For `nix build` & `nix run`:
        defaultPackage = naersk'.buildPackage {
          src = ./.;
        };

        devShell =
          if system == "x86_64-linux" then
            pkgs.mkShell {
              packages = commonPackages;
              nativeBuildInputs = commonNativeBuildInputs ++ (with pkgs; [
                libiconv
                libevdev
                rust-bindgen-unwrapped
                stdenv
                llvmPackages.bintools
                llvmPackages_16.lld
                gccMultiStdenv
                pkgsi686Linux.glibc
                clang
              ]);
            }
          else if system == "aarch64-darwin" then
            pkgs.mkShell {
              packages = commonPackages;
              nativeBuildInputs = commonNativeBuildInputs ++ (with pkgs; [
                libiconv
                darwin.apple_sdk.frameworks.Security
              ]);
            }
          else
            throw "Unsupported system: ${system}";
      });
}
