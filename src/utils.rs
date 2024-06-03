use std::io::{self, Write};

pub fn get_input(prompt: &Option<String>, default_value: &str) -> String {
    print!("{}: ", prompt.as_deref().unwrap_or(default_value));
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");
    let input = input.trim();

    if input.is_empty() {
        default_value.to_string()
    } else {
        input.to_string()
    }
}

pub fn get_hypervisor_input(input: &Option<String>) -> String {
    loop {
        let value = get_input(input, "Hypervisor (cloud-hypervisor, qemu, firecracker)");
        match value.as_str() {
            "cloud-hypervisor" | "qemu" | "firecracker" => return value,
            _ => println!(
                "Invalid input. Please enter 'cloud-hypervisor', 'qemu', or 'firecracker'."
            ),
        }
    }
}

pub fn get_input_bool(prompt: &str) -> bool {
    loop {
        print!("{} (yes/no): ", prompt);
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input");
        let input = input.trim().to_lowercase();

        match input.as_str() {
            "yes" | "y" => return true,
            "no" | "n" => return false,
            _ => println!("Invalid input. Please enter 'yes' or 'no'."),
        }
    }
}
