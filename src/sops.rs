use serde::Serialize;
use std::borrow::Cow;
use std::fs::File;
use std::io::Write;
use std::process::{exit, Command};

// Function to encrypt a file using sops
fn encrypt_file_with_sops(file_path: &str, enc_file_path: &str) {
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

// Function to create and encrypt a generic configuration file
fn create_and_encrypt_config<T: Serialize>(file_path: &str, enc_file_path: &str, config: &T) {
    // Serialize the configuration struct to YAML
    let yaml_content = serde_yaml::to_string(config).expect("Failed to serialize config");

    // Write the serialized YAML to a file
    let mut file = File::create(file_path).expect("Unable to create file");
    file.write_all(yaml_content.as_bytes())
        .expect("Unable to write data");

    // Encrypt the file using sops
    encrypt_file_with_sops(file_path, enc_file_path);

    println!(
        "{} file has been created and encrypted successfully.",
        enc_file_path
    );
}

// Struct to represent Tailscale configuration
#[derive(Serialize)]
struct TailscaleConfig {
    tailscale: Tailscale,
}

// Struct to represent individual Tailscale fields
#[derive(Serialize)]
struct Tailscale {
    auth_key: String,
    api_key: String,
    tailnet: String,
    exit_node: String,
}

// Function to generate and encrypt Tailscale configuration file
pub fn tailscale(
    auth_key: Option<&str>,
    api_key: Option<&str>,
    tailnet: Option<&str>,
    exit_node: Option<&str>,
) {
    // Get values from options or prompt user for input
    let auth_key = get_or_prompt(auth_key, "Enter Tailscale auth key");
    let api_key = get_or_prompt(api_key, "Enter Tailscale API key");
    let tailnet = get_or_prompt(tailnet, "Enter Tailscale tailnet");
    let exit_node = get_or_prompt(exit_node, "Enter Tailscale exit node");

    // Create the configuration struct
    let config = TailscaleConfig {
        tailscale: Tailscale {
            auth_key: auth_key.into_owned(),
            api_key: api_key.into_owned(),
            tailnet: tailnet.into_owned(),
            exit_node: exit_node.into_owned(),
        },
    };

    // Create and encrypt the configuration file
    create_and_encrypt_config("tailscale.yaml", "tailscale.enc.yaml", &config);
}

// Struct to represent AWS configuration
#[derive(Serialize)]
struct AwsConfig {
    aws: Aws,
}

// Struct to represent individual AWS fields
#[derive(Serialize)]
struct Aws {
    access_key_id: String,
    secret_access_key: String,
    region: String,
}

// Function to generate and encrypt AWS configuration file
pub fn aws(access_key_id: Option<&str>, secret_access_key: Option<&str>, region: Option<&str>) {
    // Get values from options or prompt user for input
    let access_key_id = get_or_prompt(access_key_id, "Enter AWS access key ID");
    let secret_access_key = get_or_prompt(secret_access_key, "Enter AWS secret access key");
    let region = get_or_prompt(region, "Enter AWS region");

    // Create the configuration struct
    let config = AwsConfig {
        aws: Aws {
            access_key_id: access_key_id.into_owned(),
            secret_access_key: secret_access_key.into_owned(),
            region: region.into_owned(),
        },
    };

    // Create and encrypt the configuration file
    create_and_encrypt_config("aws.yaml", "aws.enc.yaml", &config);
}

// Struct to represent Cloudflare configuration
#[derive(Serialize)]
struct CloudflareConfig {
    cloudflare: Cloudflare,
}

// Struct to represent individual Cloudflare fields
#[derive(Serialize)]
struct Cloudflare {
    dns_api_token: String,
}

// Function to generate and encrypt Cloudflare configuration file
pub fn cloudflare(dns_api_token: Option<&str>) {
    // Get value from option or prompt user for input
    let dns_api_token = get_or_prompt(dns_api_token, "Enter Cloudflare DNS API token");

    // Create the configuration struct
    let config = CloudflareConfig {
        cloudflare: Cloudflare {
            dns_api_token: dns_api_token.into_owned(),
        },
    };

    // Create and encrypt the configuration file
    create_and_encrypt_config("cloudflare.yaml", "cloudflare.enc.yaml", &config);
}

// Struct to represent Wiki.js configuration
#[derive(Serialize)]
struct WikijsConfig {
    wikijs: Wikijs,
}

// Struct to represent individual Wiki.js fields
#[derive(Serialize)]
struct Wikijs {
    port: u16,
    db_type: String,
    db_host: String,
    db_port: u16,
    db_name: String,
    db_user: String,
    db_password: String,
}

// Function to generate and encrypt Wiki.js configuration file
pub fn wikijs(db_user: Option<&str>, db_password: Option<&str>) {
    // Get values from options or prompt user for input
    let db_user = get_or_prompt(db_user, "Enter Wiki.js DB user");
    let db_password = get_or_prompt(db_password, "Enter Wiki.js DB password");

    // Create the configuration struct with fixed and user-provided values
    let config = WikijsConfig {
        wikijs: Wikijs {
            port: 3000,
            db_type: "postgres".to_string(),
            db_host: "localhost".to_string(),
            db_port: 5432,
            db_name: "wiki".to_string(),
            db_user: db_user.into_owned(),
            db_password: db_password.into_owned(),
        },
    };

    // Create and encrypt the configuration file
    create_and_encrypt_config("wikijs.yaml", "wikijs.enc.yaml", &config);
}

// Function to get value or prompt user for input
fn get_or_prompt<'a>(opt: Option<&'a str>, prompt: &str) -> Cow<'a, str> {
    match opt {
        Some(val) => Cow::Borrowed(val),
        None => Cow::Owned(prompt_for_input(prompt)),
    }
}

// Function to prompt user for input
fn prompt_for_input(message: &str) -> String {
    use std::io::{self, Write};
    print!("{}: ", message);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");
    input.trim().to_string()
}
