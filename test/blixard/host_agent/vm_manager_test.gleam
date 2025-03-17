//// test/blixard/host_agent/vm_manager_test.gleam

import blixard/host_agent/types
import blixard/host_agent/vm_manager
import gleam/dict
import gleam/list
import gleam/option.{None, Some}
import gleeunit
import gleeunit/should

pub fn main() {
  gleeunit.main()
}

// Helper function to safely get element at index
fn get_at(items: List(a), index: Int) -> Result(a, Nil) {
  case index {
    0 -> list.first(items)
    _ -> list.drop(items, index) |> list.first
  }
}

// Test parsing microvm list output
pub fn parse_microvm_list_output_test() {
  // Sample output from microvm -l
  let sample_output =
    "
web-server: current(abcdef), not booted: systemctl start microvm@web-server.service
database: current(123456): systemctl restart microvm@database.service
cache: outdated(789abc), rebuild(defghi) and reboot: microvm -Ru cache
"

  let result = vm_manager.parse_microvm_list_output(sample_output)

  // Should find 3 VMs
  list.length(result) |> should.equal(3)

  // Check first VM with type annotation
  let web_server: types.MicroVMStatus = get_at(result, 0) |> should.be_ok
  web_server.name |> should.equal("web-server")
  web_server.is_running |> should.equal(False)
  web_server.is_outdated |> should.equal(False)
  web_server.system_version |> should.equal(Some("abcdef"))

  // Check second VM with type annotation
  let database: types.MicroVMStatus = get_at(result, 1) |> should.be_ok
  database.name |> should.equal("database")
  database.is_running |> should.equal(True)
  database.is_outdated |> should.equal(False)
  database.system_version |> should.equal(Some("123456"))

  // Check third VM with type annotation
  let cache: types.MicroVMStatus = get_at(result, 2) |> should.be_ok
  cache.name |> should.equal("cache")
  cache.is_running |> should.equal(True)
  // Not explicitly "not booted"
  cache.is_outdated |> should.equal(True)
  cache.system_version |> should.equal(Some("789abc"))
}

// Test parsing individual lines
pub fn parse_microvm_list_line_test() {
  // Test a VM that's not running
  let line1 =
    "web-server: current(abcdef), not booted: systemctl start microvm@web-server.service"
  let result1: types.MicroVMStatus =
    vm_manager.parse_microvm_list_line(line1) |> should.be_some
  result1.name |> should.equal("web-server")
  result1.is_running |> should.equal(False)
  result1.system_version |> should.equal(Some("abcdef"))

  // Test a running VM
  let line2 =
    "database: current(123456): systemctl restart microvm@database.service"
  let result2: types.MicroVMStatus =
    vm_manager.parse_microvm_list_line(line2) |> should.be_some
  result2.name |> should.equal("database")
  result2.is_running |> should.equal(True)
  result2.system_version |> should.equal(Some("123456"))

  // Test an outdated VM
  let line3 =
    "cache: outdated(789abc), rebuild(defghi) and reboot: microvm -Ru cache"
  let result3: types.MicroVMStatus =
    vm_manager.parse_microvm_list_line(line3) |> should.be_some
  result3.name |> should.equal("cache")
  result3.is_outdated |> should.equal(True)
  result3.system_version |> should.equal(Some("789abc"))

  // Test invalid line
  let line4 = "invalid line without colon"
  let result4 = vm_manager.parse_microvm_list_line(line4) |> should.be_none
}
