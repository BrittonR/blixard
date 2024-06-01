use std::io::stdin;

pub fn get_input(value: &Option<String>, prompt: &str) -> String {
    match value {
        Some(v) => v.clone(),
        None => {
            println!("Enter {}:", prompt);
            let mut input = String::new();
            stdin().read_line(&mut input).expect("Failed to read line");
            input.trim().to_string()
        }
    }
}

pub fn get_input_bool(prompt: &str) -> bool {
    println!("{} (yes/no):", prompt);
    let mut input = String::new();
    stdin().read_line(&mut input).expect("Failed to read line");
    input.trim().to_lowercase() == "yes"
}
