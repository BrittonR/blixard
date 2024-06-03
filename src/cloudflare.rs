use std::fs::File;
use std::io::Write;
use std::process::{exit, Command};

use serde::Serialize;

#[derive(Serialize)]
struct CloudflareConfig {
    cloudflare: Cloudflare,
}

#[derive(Serialize)]
struct Cloudflare {
    dns_api_token: String,
}

pub fn generate_cloudflare_config(dns_api_token: &str) {
    let config = CloudflareConfig {
        cloudflare: Cloudflare {
            dns_api_token: dns_api_token.to_string(),
        },
    };

    let yaml_content = serde_yaml::to_string(&config).expect("Failed to serialize config");

    let file_path = "cloudflare.yaml";
    let mut file = File::create(file_path).expect("Unable to create file");
    file.write_all(yaml_content.as_bytes())
        .expect("Unable to write data");

    encrypt_file_with_sops(file_path);

    println!("cloudflare.enc.yaml file has been created and encrypted successfully.");
}

fn encrypt_file_with_sops(file_path: &str) {
    let enc_file_path = "cloudflare.enc.yaml";
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
