use crate::utils::{
    apply_regex_patterns, check_and_generate_ssh_key_if_needed,
    generate_ssh_key_pair_and_save_sops, get_input,
};
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{self, Write};
/// Function to generate an Nginx configuration and add it to the servicesConfig list
///
/// # Arguments
///
/// * `name` - The name of the service
/// * `port` - The port number the service listens on
/// * `ip` - The IP address of the service
/// * `enabled` - A boolean indicating if the service is enabled
pub fn generate_nginx_config(name: &str, port: &u16, ip: &str, enabled: &bool) {
    let acme_email = get_input(&None, "Enter the ACME email");
    let domain_name = get_input(&None, "Enter the domain name");
    let dns_provider = get_input(&None, "Enter the DNS provider");
    let sops_secret_name = get_input(&None, "Enter the SOPS secret name");
    let sops_file_path = get_input(&None, "Enter the SOPS file path");

    let new_service_config = format!(
        "{{ name = \"{}\"; port = {}; ip = \"{}\"; enabled = {}; }}",
        name, port, ip, enabled
    );

    let file_path = "modules/nginx.nix";
    if !std::path::Path::new(file_path).exists() {
        // Generate the nginx.nix file if it doesn't exist
        let nginx_nix_content = format!(
            r#"
            {{ config, pkgs, lib, ... }}: let
            # General configuration settings
            acmeEmail = "{acme_email}";  # Email for ACME registrations
            domainName = "{domain_name}";         # Primary domain name

            # Dynamic DNS and SOPS configuration
            dnsProvider = "{dns_provider}";        # DNS provider for ACME
            sopsSecretName = "{sops_secret_name}"; # Name of the SOPS secret for DNS credentials
            sopsFilePath = "{sops_file_path}"; # Path to the SOPS encrypted file

            # List of services with their configurations
            servicesConfig = [
                {{ name = "{name}"; port = {port}; ip = "{ip}"; enabled = {enabled}; }}
            ];

            # Function to generate ACME certificate configuration
            generateCertConfig = domain: {{
                group = "acme";                                 # Group for ACME-related configurations
                email = acmeEmail;                              # Email for ACME registration
                dnsProvider = dnsProvider;                      # DNS provider (Cloudflare)
                credentialsFile = config.sops.secrets."${{sopsSecretName}}".path; # Path to the DNS provider credentials
                extraDomainNames = [];                          # Extra domain names, if any
            }};

            # Function to generate NGINX virtual host configuration
            generateNginxConfig = domain: port: ip: {{
                forceSSL = true;                                 # Enforce SSL
                enableACME = true;                               # Enable ACME for automatic SSL certificates
                acmeRoot = null;                                 # ACME root directory
                locations."/" = {{
                    proxyPass = "http://${{ip}}:${{toString port}}";  # Proxy pass configuration to the service
                }};
            }};

            # Filter out enabled services
            enabledServices = filter (service: service.enabled) servicesConfig;

            # Generate ACME certificate configurations for enabled services
            acmeCerts = map (service: {{
                name = service.name;
                certConfig = generateCertConfig service.name;
            }}) enabledServices;
            # Generate NGINX virtual hosts configurations for enabled services
  nginxVirtualHosts = mapAttrs (service: serviceConfig: {{
    inherit (serviceConfig) config;
    "${{serviceConfig.name}}.${{domainName}}" = generateNginxConfig serviceConfig.name serviceConfig.port serviceConfig.ip;
  }}) enabledServices;            in {{
                # SOPS secrets configuration
                sops.secrets."${{sopsSecretName}}" = {{
                    sopsFile = sopsFilePath;  # Path to the SOPS encrypted file
                }};

                # ACME configuration for automatic SSL certificates
                security.acme = {{
                    email = acmeEmail;              # Email for ACME registration
                    acceptTerms = true;             # Accept ACME terms of service
                    useRoot = true;                 # Use root for ACME operations
                    defaults.postRun = "chmod 777 -R /var/lib/acme";  # Post-run command to change permissions
                    certs = lib.mapAttrs (service: config: {{
                        inherit (config) group email dnsProvider credentialsFile extraDomainNames;
                    }}) (lib.genAttrs (map (service: service.name + "." + domainName) enabledServices) (s: acmeCerts.${{s}}));
                }};

                # NGINX service configuration
                services.nginx = {{
                    enable = true;                   # Enable NGINX service
                    recommendedTlsSettings = true;   # Recommended TLS settings
                    recommendedProxySettings = true; # Recommended proxy settings
                    virtualHosts = lib.mkMerge (nginxVirtualHosts); # Merge all virtual host configurations
                }};

                # Add 'acme' group to the 'nginx' user
                users.users.nginx.extraGroups = ["acme"];

                # Systemd service to set correct permissions for ACME directory
                systemd.services.acme-chmod = {{
                    wantedBy = ["multi-user.target"]; # Target to start this service
                    after = acmeTargets;              # Run after ACME targets
                    requires = acmeTargets;           # Requires ACME targets
                    serviceConfig = {{
                        Type = "oneshot";               # One-shot service
                        ExecStart = "${{pkgs.coreutils}}/bin/chmod 777 -R /var/lib/acme"; # Command to set permissions
                    }};
                }};
            }}"#
        );

        let mut file = File::create(file_path).expect("Unable to create file");
        file.write_all(nginx_nix_content.as_bytes())
            .expect("Unable to write data");

        println!("nginx.nix file has been created and populated successfully.");
    } else {
        // Add the new service config to the existing nginx.nix file
        let patterns_and_inserts = [(
            r"(servicesConfig\s*=\s*\[)",
            format!("\n    {}", new_service_config),
        )];

        match apply_regex_patterns(file_path, file_path, &patterns_and_inserts) {
            Ok(_) => println!(
                "Nginx configuration for service '{}' added to {}",
                name, file_path
            ),
            Err(e) => eprintln!("Failed to add Nginx configuration: {}", e),
        }
    }
}
