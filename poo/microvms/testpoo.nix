
        { system, microvm, nixpkgs }:

        mkMicrovmConfig = {
            name = "testpoo";
            hostname = "testpoo";
            volumes = [ {
                mountPoint = "/var";
                image = "var.img";
                size = 256;
            } ];
            shares = [ {
                proto = "9p";
                tag = "ro-store";
                source = "/nix/store";
                mountPoint = "/nix/.ro-store";
            } ];
            hypervisor = "cloud-hypervisor";
            socket = "control.socket";
            extraModules = [../../modules/tailscale.nix];
        };

        mkMicrovmConfig
        