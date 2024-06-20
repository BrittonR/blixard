{inputs}:
    let inherit (inputs) self nixpkgs nixos-generators sops-nix deploy-rs;

    mkNode = server: ip: fast: {
      hostname = "${server}";
      fastConnection = fast;
      activationTimeout = 600;
      confirmTimeout = 600;
      sshOpts = [ "-i" "/home/admin/.ssh/admin" ];
      autoRollback = true;
      profiles.system.path =
        deploy-rs.lib.x86_64-linux.activate.nixos
          self.nixosConfigurations."${server}";
    };

    in {
      user = "admin";
      sshUser = "admin";
      nodes = {
        tower = mkNode "tower" "10.0.0.2" true;
      
    testpoo = mkNode "testpoo" "testpoo" true;};
    };
}