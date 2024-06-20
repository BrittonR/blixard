{ pkgs, config, ... }: {
  sops.secrets."tailscale/authKey" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  sops.secrets."tailscale/apiKey" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  sops.secrets."tailscale/tailnet" = {
    sopsFile = "${inputs.self}/sops/tailscale.enc.yaml";
  };
  microvm.shares = [{
    tag = "tailscale";
    source = "/run/secrets/tailscale";
    mountPoint = "/dev/tailscale";
    proto = "virtiofs";
  }];
}
