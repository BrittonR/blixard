{
  microvm = {
    vcpu = 2;
    mem = 512;
    
    interfaces = [{
      type = "tap";
      id = "vm0";
      mac = "02:00:00:00:00:01";
    }];
    
    shares = [{
      source = "/nix/store";
      mountPoint = "/nix/.ro-store";
      tag = "ro-store";
      proto = "virtiofs";
    }];
    
    kernel = builtins.fetchurl {
      url = "https://github.com/NixOS/nixpkgs/raw/master/pkgs/os-specific/linux/kernel/linux-5.10.nix";
      sha256 = "0000000000000000000000000000000000000000000000000000";
    };
    
    initrd = null;
  };
  
  # NixOS configuration for the VM
  imports = [ ];
  
  networking.hostName = "test-vm";
  
  system.stateVersion = "23.11";
}