{
  description = "NixOS configuration with flakes";

  inputs = {
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-stable.url = "github:NixOS/nixpkgs/nixos-24.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    nix-darwin = {
      url = "github:LnL7/nix-darwin";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nixpkgs-darwin.url = "github:nixos/nixpkgs/nixpkgs-23.11-darwin";

    sops-nix.url = "github:Mic92/sops-nix";

    home-manager.url = "github:nix-community/home-manager";
    home-manager.inputs.nixpkgs.follows = "nixpkgs";

    microvm.url = "github:astro/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";

    disko.url = "github:nix-community/disko";
    disko.inputs.nixpkgs.follows = "nixpkgs";

    terranix.url = "github:terranix/terranix";

    nixos-hardware.url = "github:NixOS/nixos-hardware/master";

    deploy-rs.url = "github:serokell/deploy-rs";
  };

  outputs = inputs@{ self, ... }:
    let
      system = "x86_64-linux";

      # Import the base configuration
      baseConfig =
        import ./microvms/base-microvm.nix { inherit system inputs; };

      # Import microVM configurations
      microvmConfigs =
        import ./configs/microvmConfigs.nix { inherit baseConfig; };

      # Import other NixOS configurations
      otherNixosConfigs =
        import ./configs/otherNixosConfigs.nix { inherit inputs system; };

      # Combine both lists
      allConfigs = microvmConfigs ++ otherNixosConfigs;

      # Generate nixosConfigurations and packages attributes
      nixosConfigurations = builtins.listToAttrs (map (vm: {
        name = vm.name;
        value = vm.config;
      }) allConfigs);

      packages = builtins.listToAttrs (map (vm: {
        name = vm.name;
        value = vm.config.config.microvm.declaredRunner or null;
      }) microvmConfigs) // {
        default = nixosConfigurations.my-microvm.config.microvm.declaredRunner;
      };

    in { inherit nixosConfigurations packages; };
}
