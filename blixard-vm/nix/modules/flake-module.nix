{ self, inputs, ... }:

{
  imports = [
    ./core/microvm.nix
  ];
  
  flake = {
    # Export the flake module so other flakes can use it
    flakeModule = ./default.nix;
    
    # Export individual modules for direct import
    nixosModules = {
      blixard-microvm = ./core/microvm.nix;
    };
  };
}