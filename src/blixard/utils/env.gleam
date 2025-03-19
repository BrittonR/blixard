//// src/blixard/utils/env.gleam

///
/// Environment variable utilities
import gleam/erlang
import gleam/option.{type Option, None, Some}
import gleam/result

// Get an environment variable or return a default
pub fn get_env(name: String, default: String) -> String {
  case os_get_env(name) {
    Some(value) -> value
    None -> default
  }
}

// Set an environment variable
pub fn set_env(name: String, value: String) -> Nil {
  let _ = os_set_env(name, value)
  Nil
}

// Get an environment variable as an Option
@external(erlang, "os", "getenv")
fn os_get_env(name: String) -> Option(String)

// Set an environment variable
@external(erlang, "os", "putenv")
fn os_set_env(name: String, value: String) -> Nil
