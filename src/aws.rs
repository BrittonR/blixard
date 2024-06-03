use std::fs::File;
use std::io::Write;
use std::process::{exit, Command};

use serde::Serialize;

#[derive(Serialize)]
struct AwsConfig {
    aws: Aws,
}

#[derive(Serialize)]
struct Aws {
    access_key_id: String,
    secret_access_key: String,
    region: String,
}

pub fn generate_aws_config(access_key_id: &str, secret_access_key: &str, region: &str) {
    let config = AwsConfig {
        aws: Aws {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
            region: region.to_string(),
        },
    };

    let yaml_content = serde_yaml::to_string(&config).expect("Failed to serialize config");

    let file_path = "aws.yaml";
    let mut file = File::create(file_path).expect("Unable to create file");
    file.write_all(yaml_content.as_bytes())
        .expect("Unable to write data");

    encrypt_file_with_sops(file_path);

    println!("aws.enc.yaml file has been created and encrypted successfully.");
}

fn encrypt_file_with_sops(file_path: &str) {
    let enc_file_path = "aws.enc.yaml";
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
