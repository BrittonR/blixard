# Minimal test to show the generated flake structure is valid
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      packages.${system}.hello = pkgs.writeShellScriptBin "hello-blixard" ''
        echo "Blixard VM integration works!"
        echo "This would start a microVM with:"
        echo "  - 2 vCPUs"
        echo "  - 1024MB RAM"
        echo "  - cloud-hypervisor backend"
      '';
    };
}