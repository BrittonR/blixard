use pnet::datalink::{self, NetworkInterface};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize)]
struct SopsAdminKey {
    ssh_public_key: String,
    ssh_private_key: String,
}

/// Helper function to get user input from stdin.
pub fn get_input(prompt: &Option<String>, message: &str) -> String {
    // Use provided prompt or default message
    let prompt = prompt.as_deref().unwrap_or(message);
    print!("{}: ", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");
    input.trim().to_string()
}

/// Helper function to get boolean input from user.
pub fn get_input_bool(message: &str) -> bool {
    loop {
        let input = get_input(&None, message);
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => return true,
            "n" | "no" => return false,
            _ => println!("Please enter 'y', 'yes', 'n', or 'no'."),
        }
    }
}

/// Helper function to get hypervisor choice from user.
pub fn get_hypervisor_input(hypervisor: Option<&str>) -> String {
    let hypervisor = hypervisor
        .unwrap_or("What hypervisor would you like to use? (cloud-hypervisor, qemu, firecracker)");
    loop {
        let input = get_input(
            &Some(hypervisor.to_string()),
            "Hypervisor (cloud-hypervisor, qemu, firecracker)",
        );
        let input_trimmed = input.trim();
        if input_trimmed.is_empty() {
            return "cloud-hypervisor".to_string();
        }
        match input_trimmed.to_lowercase().as_str() {
            "cloud-hypervisor" | "qemu" | "firecracker" => return input_trimmed.to_string(),
            _ => println!("Please enter 'cloud-hypervisor', 'qemu', or 'firecracker'."),
        }
    }
}

/// Helper function to select a network interface from available options.
pub fn select_network_interface() -> String {
    let interfaces = datalink::interfaces();
    println!("Available network interfaces:");

    // Print available interfaces with their IP addresses
    for (index, interface) in interfaces.iter().enumerate() {
        let ip_addresses: Vec<String> =
            interface.ips.iter().map(|ip| ip.ip().to_string()).collect();
        println!(
            "{}: {} [{}]",
            index + 1,
            interface.name,
            ip_addresses.join(", ")
        );
    }

    // Additional options for default and custom interfaces
    println!(
        "{}: Use default AWS EC2 external interface (eth0)",
        interfaces.len() + 1
    );
    println!("{}: Use enP2p4s0 interface", interfaces.len() + 2);
    println!("{}: Enter a custom interface name", interfaces.len() + 3);

    // Loop until a valid interface is selected
    loop {
        let input = get_input(
            &None,
            "Select the interface number to use as the external interface",
        );
        if let Ok(index) = input.trim().parse::<usize>() {
            if index > 0 && index <= interfaces.len() {
                return interfaces[index - 1].name.clone();
            } else if index == interfaces.len() + 1 {
                return "eth0".to_string();
            } else if index == interfaces.len() + 2 {
                return "enP2p4s0".to_string();
            } else if index == interfaces.len() + 3 {
                return get_input(&None, "Enter the custom interface name");
            }
        }
        println!("Invalid selection. Please enter a valid interface number.");
    }
}

/// Generate SSH key pair and save it encrypted with SOPS.
pub fn generate_ssh_key_pair_and_save_sops() -> String {
    let key_name = "admin_ssh_key";

    // Generate SSH key pair
    let output = Command::new("ssh-keygen")
        .args(&["-t", "ed25519", "-f", key_name, "-N", "", "-q"])
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to generate SSH key pair");

    // Read generated keys
    let pub_key_path = format!("{}.pub", key_name);
    let priv_key_path = key_name;
    let pub_key = std::fs::read_to_string(&pub_key_path).expect("Failed to read public key");
    let priv_key = std::fs::read_to_string(&priv_key_path).expect("Failed to read private key");

    // Create SOPSAdminKey struct
    let admin_key = SopsAdminKey {
        ssh_public_key: pub_key.trim().to_string(),
        ssh_private_key: priv_key.trim().to_string(),
    };

    // Serialize keys to YAML
    let yaml_content = serde_yaml::to_string(&admin_key).expect("Failed to serialize keys");

    // Write YAML to SOPS file
    let sops_file_path = "admin_ssh_key.enc.yaml";
    let mut sops_file = File::create(&sops_file_path).expect("Failed to create SOPS file");
    sops_file
        .write_all(yaml_content.as_bytes())
        .expect("Failed to write to SOPS file");

    // Encrypt SOPS file
    let status = Command::new("sops")
        .args(&["--encrypt", "--in-place", &sops_file_path])
        .status()
        .expect("Failed to run sops");

    // Check encryption status
    if !status.success() {
        eprintln!("Failed to encrypt the SOPS file");
        std::process::exit(1);
    }

    // Clean up generated key files
    std::fs::remove_file(pub_key_path).expect("Failed to remove public key file");
    std::fs::remove_file(priv_key_path).expect("Failed to remove private key file");

    admin_key.ssh_public_key
}

/// Read and decrypt SSH public key from SOPS file.
pub fn read_sops_file(file_path: &str) -> String {
    // Decrypt SOPS file
    let output = Command::new("sops")
        .args(&["--decrypt", file_path])
        .output()
        .expect("Failed to run sops");

    // Check decryption status
    if !output.status.success() {
        eprintln!("Failed to decrypt the SOPS file");
        std::process::exit(1);
    }

    // Parse decrypted YAML content
    let content: Value = serde_yaml::from_slice(&output.stdout).expect("Failed to parse YAML");
    content["ssh_public_key"].as_str().unwrap().to_string()
}

/// Check if SOPS file exists, and generate SSH key pair if needed.
pub fn check_and_generate_ssh_key_if_needed() -> String {
    let sops_file_path = "sops/admin_ssh_key.enc.yaml";

    // Check if SOPS file exists
    if std::path::Path::new(sops_file_path).exists() {
        // Attempt to decrypt SOPS file
        match Command::new("sops")
            .args(&["--decrypt", sops_file_path])
            .output()
        {
            Ok(output) => {
                // Return SSH public key if decryption successful
                if output.status.success() {
                    let content: Value =
                        serde_yaml::from_slice(&output.stdout).expect("Failed to parse YAML");
                    return content["ssh_public_key"].as_str().unwrap().to_string();
                }
            }
            Err(_) => {}
        }
    }

    // Generate SSH key pair and save with SOPS
    generate_ssh_key_pair_and_save_sops()
}
/// Function to apply regex patterns and insert texts into the input file content, then write the result to the output file
///
/// # Arguments
///
/// * `input_file` - A string slice that holds the name of the input file
/// * `output_file` - A string slice that holds the name of the output file
/// * `patterns_and_inserts` - A slice of tuples containing regex patterns and their corresponding insert texts
///
/// # Returns
///
/// This function returns a `Result` with unit type on success and `io::Error` on failure
pub fn apply_regex_patterns(
    input_file: &str,
    output_file: &str,
    patterns_and_inserts: &[(&str, String)],
) -> io::Result<()> {
    // Open the input file and read its contents into a string
    let mut file = File::open(input_file)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;

    // Apply each regex pattern to the text and insert the corresponding insert text
    let modified_text = patterns_and_inserts
        .iter()
        .fold(text, |acc, (pattern, insert_text)| {
            let re = Regex::new(pattern).unwrap();
            re.replace_all(&acc, |caps: &regex::Captures| {
                // Capture the matched text
                let cap_text = caps[0].to_string();
                let len = cap_text.len();
                // If the captured text length is greater than 2, split it and insert the text in between
                if len > 2 {
                    let (main_text, last_two) = cap_text.split_at(len - 2);
                    format!("{}{}{}", main_text, insert_text, last_two)
                } else {
                    // If the captured text length is 2 or less, simply concatenate the captured text and insert text
                    format!("{}{}", cap_text, insert_text)
                }
            })
            .to_string()
        });

    // Open (or create) the output file and write the modified text into it
    let mut output_file = File::create(output_file)?;
    output_file.write_all(modified_text.as_bytes())?;

    println!("Modified text written to {:?}", output_file);
    Ok(())
}
