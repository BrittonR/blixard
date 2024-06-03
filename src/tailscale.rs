use std::fs::File;
use std::io::Write;
use std::process::{exit, Command};

use serde::Serialize;

#[derive(Serialize)]
struct TailscaleConfig {
    tailscale: Tailscale,
}

#[derive(Serialize)]
struct Tailscale {
    authKey: String,
    apiKey: String,
    tailnet: String,
    exitNode: String,
}

pub fn generate_tailscale_config(auth_key: &str, api_key: &str, tailnet: &str, exit_node: &str) {
    let config = TailscaleConfig {
        tailscale: Tailscale {
            authKey: auth_key.to_string(),
            apiKey: api_key.to_string(),
            tailnet: tailnet.to_string(),
            exitNode: exit_node.to_string(),
        },
    };

    let yaml_content = serde_yaml::to_string(&config).expect("Failed to serialize config");

    let file_path = "tailscale.yaml";
    let mut file = File::create(file_path).expect("Unable to create file");
    file.write_all(yaml_content.as_bytes())
        .expect("Unable to write data");

    encrypt_file_with_sops(file_path);

    println!("tailscale.enc.yaml file has been created and encrypted successfully.");
}

fn encrypt_file_with_sops(file_path: &str) {
    let enc_file_path = "tailscale.enc.yaml";
    let status = Command::new("sops")
        .args(&["--encrypt", "--output", enc_file_path, file_path])
        .status()
        .expect("Failed to run sops");

    if !status.success() {
        eprintln!("Failed to encrypt the file with sops");
        exit(1);
    }

    std::fs::remove_file(file_path).expect("Failed to remove the unencrypted file");
}
