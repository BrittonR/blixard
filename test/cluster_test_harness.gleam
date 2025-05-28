// test/cluster_test_harness.gleam
import gleam/erlang
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import gleam/string
import khepri_store
import node_manager
import shellout

pub type TestNode {
  TestNode(
    name: String,
    os_pid: String,
    // OS process ID instead of Erlang pid
    started_at: Int,
  )
}

pub type ClusterTestHarness {
  ClusterTestHarness(nodes: List(TestNode), primary_node: String)
}

/// Start a test cluster with N nodes
pub fn start_test_cluster(node_count: Int) -> Result(ClusterTestHarness, String) {
  io.println(
    "Starting test cluster with " <> int.to_string(node_count) <> " nodes",
  )

  // Start primary node first
  let primary_name = "blixard_test_primary"
  case start_test_node(primary_name, None) {
    Ok(primary_node) -> {
      // Wait for primary to stabilize
      process.sleep(3000)

      // Start secondary nodes
      let secondary_results =
        list.range(1, node_count - 1)
        |> list.map(fn(i) {
          let name = "blixard_test_node_" <> int.to_string(i)
          start_test_node(name, Some(primary_name <> "@127.0.0.1"))
        })

      // Collect successful nodes
      let all_nodes = [
        primary_node,
        ..list.filter_map(secondary_results, fn(r) {
          case r {
            Ok(node) -> Ok(node)
            Error(_) -> Error(Nil)
          }
        })
      ]

      Ok(ClusterTestHarness(
        nodes: all_nodes,
        primary_node: primary_name <> "@127.0.0.1",
      ))
    }
    Error(e) -> Error("Failed to start primary node: " <> e)
  }
}

/// Start a single test node using the existing service_manager
fn start_test_node(
  name: String,
  primary: option.Option(String),
) -> Result(TestNode, String) {
  io.println("Starting test node: " <> name)

  // Build the command to start the node
  let base_cmd = "gleam run -m service_manager -- --join-cluster"
  let cmd = case primary {
    None -> base_cmd
    // Primary node
    Some(primary_node) -> base_cmd <> " " <> primary_node
    // Secondary node
  }

  // Add node name and cookie
  let full_cmd =
    "erl -name "
    <> name
    <> "@127.0.0.1 -setcookie test_cookie -noshell -s "
    <> cmd
    <> " &"

  // Execute in background and capture PID
  case
    shellout.command(
      run: "sh",
      with: ["-c", full_cmd <> " echo $!"],
      in: ".",
      opt: [],
    )
  {
    Ok(output) -> {
      let pid = string.trim(output)
      Ok(TestNode(
        name: name <> "@127.0.0.1",
        os_pid: pid,
        started_at: erlang.system_time(erlang.Millisecond),
      ))
    }
    Error(#(_, msg)) -> Error("Failed to start node: " <> msg)
  }
}

/// Stop all nodes in the test cluster
pub fn stop_test_cluster(harness: ClusterTestHarness) -> Nil {
  list.each(harness.nodes, fn(node) {
    // Kill the OS process
    let _ = shellout.command(run: "kill", with: [node.os_pid], in: ".", opt: [])
    io.println("Stopped node: " <> node.name)
  })
}

/// Check if a node is still running
pub fn is_node_running(node: TestNode) -> Bool {
  case
    shellout.command(run: "kill", with: ["-0", node.os_pid], in: ".", opt: [])
  {
    Ok(_) -> True
    Error(_) -> False
  }
}
