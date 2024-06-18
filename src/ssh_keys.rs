use openssl::rsa::Rsa;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{exit, Command};

#[derive(Serialize, Deserialize)]
struct SshKeys {
    ssh_host_rsa_key: String,
    ssh_host_rsa_key_pub: String,
    ssh_host_ed25519_key: String,
    ssh_host_ed25519_key_pub: String,
}

// Function to generate SSH keys for a project
pub fn generate_ssh_keys(project_name: &str) {
    use age::secrecy::ExposeSecret;
    use age::x25519;

    let rsa_key = "ssh_host_rsa_key";
    let ed25519_key = "ssh_host_ed25519_key";
    let key_dir = "./keys";
    let enc_file = format!("{}_keys.enc.yaml", project_name);

    // Create directory for storing keys
    fs::create_dir_all(key_dir).expect("Failed to create key directory");

    // Check if .sops.yaml exists, if not create it along with age key
    if !Path::new(".sops.yaml").exists() {
        println!(".sops.yaml file not found. Creating one.");

        let age_key_pair = x25519::Identity::generate();
        let age_public_key = age_key_pair.to_public();
        let age_secret_key = age_key_pair.to_string();

        let sops_config = format!("creation_rules:\n  - age: {}\n", age_public_key.to_string());

        let mut sops_file = File::create(".sops.yaml").expect("Failed to create .sops.yaml file");
        sops_file
            .write_all(sops_config.as_bytes())
            .expect("Failed to write to .sops.yaml file");

        let mut age_secret_file =
            File::create("age_secret_key.txt").expect("Failed to create age secret key file");
        age_secret_file
            .write_all(age_secret_key.expose_secret().as_bytes())
            .expect("Failed to write age secret key to file");

        println!(".sops.yaml file and age key created successfully.");
    }

    // Generate RSA and ED25519 SSH keys
    generate_ssh_key(rsa_key, key_dir, "rsa", 4096);
    generate_ssh_key(ed25519_key, key_dir, "ed25519", 0);

    // Read generated keys
    let rsa_key_content = fs::read_to_string(format!("{}/{}", key_dir, rsa_key)).unwrap();
    let rsa_key_pub_content = fs::read_to_string(format!("{}/{}.pub", key_dir, rsa_key)).unwrap();
    let ed25519_key_content = fs::read_to_string(format!("{}/{}", key_dir, ed25519_key)).unwrap();
    let ed25519_key_pub_content =
        fs::read_to_string(format!("{}/{}.pub", key_dir, ed25519_key)).unwrap();

    // Create SSH keys struct
    let ssh_keys = SshKeys {
        ssh_host_rsa_key: rsa_key_content,
        ssh_host_rsa_key_pub: rsa_key_pub_content,
        ssh_host_ed25519_key: ed25519_key_content,
        ssh_host_ed25519_key_pub: ed25519_key_pub_content,
    };

    // Serialize SSH keys to YAML
    let yaml_content = serde_yaml::to_string(&ssh_keys).unwrap();
    let temp_yaml_path = format!("{}/temp.yaml", key_dir);
    let mut temp_yaml_file = File::create(&temp_yaml_path).unwrap();
    temp_yaml_file.write_all(yaml_content.as_bytes()).unwrap();

    // Encrypt the YAML file with sops
    let sops_status = Command::new("sops")
        .args(&["--encrypt", "--in-place", &temp_yaml_path])
        .status()
        .expect("Failed to run sops");

    if !sops_status.success() {
        eprintln!("Failed to encrypt the file with sops");
        exit(1);
    }

    // Move the encrypted file to the final destination
    fs::rename(&temp_yaml_path, &enc_file).expect("Failed to move the encrypted file");

    // Remove the key directory
    fs::remove_dir_all(key_dir).expect("Failed to remove key directory");

    println!("Encrypted keys stored in {}", enc_file);
}

// Function to generate an individual SSH key
fn generate_ssh_key(key_name: &str, key_dir: &str, key_type: &str, bits: u32) {
    let key_path = format!("{}/{}", key_dir, key_name);
    let pub_key_path = format!("{}.pub", key_path);

    match key_type {
        "rsa" => {
            // Generate RSA key pair
            let rsa = Rsa::generate(bits).unwrap();
            let private_key_pem = rsa.private_key_to_pem().unwrap();
            let public_key_pem = rsa.public_key_to_pem().unwrap();

            // Write RSA private key to file
            let mut private_key_file = File::create(&key_path).unwrap();
            private_key_file.write_all(&private_key_pem).unwrap();

            // Write RSA public key to file
            let mut public_key_file = File::create(&pub_key_path).unwrap();
            public_key_file.write_all(&public_key_pem).unwrap();
        }
        "ed25519" => {
            // Generate ED25519 key pair using ssh-keygen
            Command::new("ssh-keygen")
                .args(&[
                    "-t", "ed25519", "-f", &key_path, "-q", "-N", "", "-C", "microvm",
                ])
                .status()
                .expect("Failed to generate ED25519 key");
        }
        _ => panic!("Unsupported key type"),
    }
}
