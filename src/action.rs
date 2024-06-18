use std::fs::{create_dir_all, File};
use std::io::{Error, Write};

/// Function to generate a GitHub Actions workflow and a corresponding script
///
/// # Arguments
///
/// * `name` - The name of the GitHub action
/// * `working_directory` - The working directory for the action script
///
/// # Returns
///
/// This function returns a `Result` with unit type on success and `Error` on failure
pub fn generate_github_action(name: &str, working_directory: &str) -> Result<(), Error> {
    // Define the content of the GitHub Actions workflow file
    let action_content = format!(
        r#"name: {name}

on: [push, pull_request]

jobs:
  {name}:
    runs-on: ubuntu-latest

    steps:
      - name: Checkout Repository
        uses: actions/checkout@v2

      - name: Install Nix
        uses: DeterminateSystems/nix-installer-action@v7

      - name: Install Nix Cache
        uses: DeterminateSystems/magic-nix-cache-action@main

      - name: Run {name} commands
        run: |
          nix develop --command bash {name}_action.sh
        working-directory: {working_directory}
"#
    );

    // Create the directory for the GitHub Actions workflow file if it doesn't exist
    let github_dir_path = ".github/workflows";
    create_dir_all(github_dir_path)?;

    // Define the file path for the GitHub Actions workflow file
    let file_path = format!("{github_dir_path}/{name}_action.yml");
    // Create and write the workflow file
    let mut file = File::create(&file_path)?;
    file.write_all(action_content.as_bytes())?;
    println!("GitHub Actions workflow file created at: {file_path}");

    // Define the content of the action script file
    let script_content = r#"#!/bin/bash
# Add your commands here
echo "Running custom actions"
"#;

    // Create the working directory for the action script if it doesn't exist
    create_dir_all(working_directory)?;

    // Define the file path for the action script file
    let script_path = format!("{working_directory}/{name}_action.sh");
    // Create and write the action script file
    let mut script_file = File::create(&script_path)?;
    script_file.write_all(script_content.as_bytes())?;
    println!("Script file created at: {script_path}");

    Ok(())
}
