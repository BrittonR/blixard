// test/cluster_verify.gleam
import cluster_discovery
import gleam/dynamic/decode
import gleam/erlang
import gleam/int
import gleam/io
import gleam/list
import gleam/result
import gleam/string
import khepri_gleam
import khepri_store

pub fn main() {
  io.println("=== Cluster Verification ===")

  // Check 1: Can we see other nodes?
  verify_cluster_nodes()

  // Check 2: Is service state consistent?
  verify_service_state()

  // Check 3: Can we access Khepri data?
  verify_khepri_access()
}

fn verify_cluster_nodes() {
  io.println("\n1. Checking cluster nodes...")

  let nodes = cluster_discovery.find_khepri_nodes()
  case list.length(nodes) {
    0 -> io.println("❌ No cluster nodes found")
    1 -> io.println("⚠️  Only one node found (standalone mode)")
    n -> io.println("✅ Found " <> int.to_string(n) <> " cluster nodes")
  }

  list.each(nodes, fn(node) { io.println("   - " <> node) })
}

fn verify_service_state() {
  io.println("\n2. Checking service state...")

  case khepri_store.list_cli_services() {
    Ok(services) -> {
      io.println(
        "✅ Found " <> int.to_string(list.length(services)) <> " services",
      )
      list.each(services, fn(service) {
        io.println(
          "   - "
          <> service.name
          <> " ("
          <> string.inspect(service.state)
          <> ") on "
          <> service.node,
        )
      })
    }
    Error(e) -> io.println("❌ Failed to list services: " <> e)
  }
}

fn verify_khepri_access() {
  io.println("\n3. Testing Khepri access...")

  // Try to write and read a test value
  let test_path = khepri_gleam.to_khepri_path("/:test/cluster_verify")
  let test_value =
    "verified_" <> int.to_string(erlang.system_time(erlang.Millisecond))

  khepri_gleam.put(test_path, test_value)

  case khepri_gleam.get(test_path) {
    Ok(value) -> {
      case decode.run(value, decode.string) {
        Ok(v) if v == test_value -> io.println("✅ Khepri read/write working")
        _ -> io.println("❌ Khepri returned unexpected value")
      }
    }
    Error(_) -> io.println("❌ Failed to read from Khepri")
  }
}
