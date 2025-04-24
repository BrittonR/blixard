// src/service_manager.gleam
import cluster_discovery
import gleam/dynamic
import gleam/erlang
import gleam/erlang/atom
import gleam/erlang/node
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/list
import gleam/option.{None, Some}
import gleam/result
import gleam/string
import khepri_gleam
import khepri_gleam_cluster
import khepri_store
import shellout
import systemd

// External function to start Erlang distribution
@external(erlang, "distribution_helper", "start_distribution")
fn start_distribution(
  node_name: String,
  hostname: String,
  cookie: String,
) -> Nil

pub fn main() -> Nil {
  // Get command-line arguments
  let args = erlang.start_arguments()

  // Process special setup commands first
  case args {
    ["--init-primary"] -> {
      start_primary_node()
      Nil
    }
    ["--init-secondary", primary_node] -> {
      start_secondary_node(primary_node)
      Nil
    }
    _ -> Nil
  }

  // Regular command processing - ensure Erlang distribution is started
  ensure_distribution()

  // Initialize Khepri (assumes it's already running in cluster mode)
  let _ = khepri_store.init()

  // Process commands
  case args {
    ["start", service] -> {
      let _ = handle_start(service)
      Nil
    }
    ["stop", service] -> {
      let _ = handle_stop(service)
      Nil
    }
    ["restart", service] -> {
      let _ = handle_restart(service)
      Nil
    }
    ["status", service] -> {
      let _ = handle_status(service)
      Nil
    }
    ["list"] -> {
      handle_list()
      Nil
    }
    ["list-cluster"] -> {
      handle_list_cluster()
      Nil
    }
    _ -> {
      print_usage()
      Nil
    }
  }
}

// Ensure we're running in distributed mode for CLI operations
fn ensure_distribution() -> Nil {
  let current_node = node.self()
  let node_name = atom.to_string(node.to_atom(current_node))

  // Check if already distributed
  case node_name == "nonode@nohost" {
    True -> {
      // Start distribution using our helper
      let name =
        "service_cli_" <> int.to_string(erlang.system_time(erlang.Millisecond))
      let hostname = "127.0.0.1"
      // Use IP address instead of "localhost"
      let cookie = "khepri_cookie"

      start_distribution(name, hostname, cookie)

      // Verify distribution started
      let new_node = node.self()
      let new_name = atom.to_string(node.to_atom(new_node))

      case new_name == "nonode@nohost" {
        True -> {
          io.println(
            "WARNING: Failed to start distribution, functionality will be limited",
          )
          Nil
        }
        False -> {
          io.println("Running as distributed node: " <> new_name)

          // Try to discover and connect to other nodes
          let _ = cluster_discovery.connect_to_all_nodes("khepri_node")
          Nil
        }
      }
    }
    False -> {
      io.println("Already running in distributed mode: " <> node_name)
      Nil
    }
  }
}

// Modify the start_primary_node function in service_manager.gleam
fn start_primary_node() -> Nil {
  io.println("Starting primary Khepri node...")

  // Start distribution
  ensure_distribution()

  // Initialize the Khepri cluster as primary
  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Primary,
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("Primary Khepri node started successfully")
      io.println("Waiting for connections...")

      // Start monitoring for new nodes
      start_node_monitor()

      // Add this line to continuously monitor replication
      start_continuous_replication_monitor()
      // In start_primary_node, after the line: start_continuous_replication_monitor()
      start_primary_write_test()

      // In start_secondary_node, after joining the cluster:

      // Keep the process running
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to start primary node: " <> err)
      Nil
    }
  }
}

fn start_secondary_node(primary_node: String) -> Nil {
  io.println("Starting secondary Khepri node...")
  io.println("Primary node: " <> primary_node)

  // Start distribution
  ensure_distribution()

  // Initialize the Khepri cluster as secondary
  let config =
    khepri_store.ClusterConfig(
      node_role: khepri_store.Secondary(primary_node),
      cookie: "khepri_cookie",
    )

  case khepri_store.init_cluster(config) {
    Ok(_) -> {
      io.println("Secondary Khepri node started successfully")
      io.println("Connected to primary node")

      // Write some test data to demonstrate replication
      // In start_secondary_node function
      // After connecting to the cluster:

      // Write join data using the existing store_service_state function
      let current_node = node.self()
      let node_name = atom.to_string(node.to_atom(current_node))
      let timestamp = int.to_string(erlang.system_time(erlang.Millisecond))

      // Create a join message
      let join_key = "join_" <> node_name
      let join_message = "Node " <> node_name <> " joined at " <> timestamp

      // Store using service state
      io.println("Writing join notification to the cluster...")
      let _ = khepri_store.store_join_notification(join_key, timestamp)

      io.println("Sent join notification to the cluster")
      // Keep the process running
      start_secondary_read_test()
      start_replication_test_cycle()
      process.sleep_forever()
    }
    Error(err) -> {
      io.println_error("Failed to start secondary node: " <> err)
      Nil
    }
  }
  // In start_secondary_node, after joining the cluster:
}

// Handler for starting a service
fn handle_start(service: String) -> Result(Nil, String) {
  io.println("Starting service: " <> service)

  case systemd.start_service(service) {
    Ok(_) -> {
      io.println("Service started successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Running)

      // Verify service is running
      case systemd.is_active(service) {
        Ok(True) -> io.println("Verified: Service is running")
        _ -> io.println("Warning: Service may not be running properly")
      }

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to start service: " <> err)

      // Record failed state
      let _ = khepri_store.store_service_state(service, khepri_store.Failed)

      Error(err)
    }
  }
}

// Handler for stopping a service
fn handle_stop(service: String) -> Result(Nil, String) {
  io.println("Stopping service: " <> service)

  case systemd.stop_service(service) {
    Ok(_) -> {
      io.println("Service stopped successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Stopped)

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to stop service: " <> err)

      // Record failed state
      let _ = khepri_store.store_service_state(service, khepri_store.Failed)

      Error(err)
    }
  }
}

// Handler for restarting a service
fn handle_restart(service: String) -> Result(Nil, String) {
  io.println("Restarting service: " <> service)

  case systemd.restart_service(service) {
    Ok(_) -> {
      io.println("Service restarted successfully")

      // Update state in Khepri
      let _ = khepri_store.store_service_state(service, khepri_store.Running)

      // Verify service is running
      case systemd.is_active(service) {
        Ok(True) -> io.println("Verified: Service is running")
        _ -> io.println("Warning: Service may not be running properly")
      }

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to restart service: " <> err)

      // Record failed state
      let _ = khepri_store.store_service_state(service, khepri_store.Failed)

      Error(err)
    }
  }
}

// Handler for getting service status
fn handle_status(service: String) -> Result(Nil, String) {
  io.println("Checking status of service: " <> service)

  // Get current state from Khepri
  case khepri_store.get_service_state(service) {
    Ok(state) -> {
      io.println("Service state in Khepri: " <> string.inspect(state))
    }
    Error(_) -> {
      io.println("Service state not found in Khepri")
    }
  }

  // Get actual systemd status
  case systemd.service_status(service) {
    Ok(status) -> {
      io.println("Systemd status:")
      io.println(status)

      // Update state in Khepri based on actual status
      case systemd.is_active(service) {
        Ok(True) -> {
          let _ =
            khepri_store.store_service_state(service, khepri_store.Running)
        }
        Ok(False) -> {
          let _ =
            khepri_store.store_service_state(service, khepri_store.Stopped)
        }
        Error(_) -> {
          let _ =
            khepri_store.store_service_state(service, khepri_store.Unknown)
        }
      }

      Ok(Nil)
    }
    Error(err) -> {
      io.println_error("Failed to get service status: " <> err)
      Error(err)
    }
  }
}

// Handler for listing services
fn handle_list() -> Nil {
  io.println("Listing all managed services:")

  case khepri_store.list_services() {
    Ok(services) -> {
      case list.length(services) {
        0 -> io.println("No services are currently managed")
        _ -> {
          io.println("\nService Status from Khepri Database:")
          io.println("--------------------------------")
          list.each(services, fn(service) {
            let #(name, state) = service
            io.println("- " <> name <> ": " <> string.inspect(state))
          })
        }
      }
    }
    Error(err) -> {
      io.println_error("Failed to list services from Khepri: " <> err)
    }
  }

  // Also list systemd services
  case systemd.list_services() {
    Ok(system_services) -> {
      io.println("\nSystemd Services Available:")
      io.println("------------------------")
      list.each(list.take(system_services, 10), fn(service) {
        io.println("- " <> service)
      })

      case list.length(system_services) > 10 {
        True ->
          io.println(
            "... and "
            <> int.to_string(list.length(system_services) - 10)
            <> " more",
          )
        False -> Nil
      }
    }
    Error(err) -> {
      io.println_error("Failed to list systemd services: " <> err)
    }
  }
}

// Handler for listing cluster nodes
fn handle_list_cluster() -> Nil {
  io.println("Connected Erlang nodes:")

  let connected_nodes = node.visible()
  case list.length(connected_nodes) {
    0 -> io.println("No connected nodes (standalone mode)")
    _ -> {
      list.each(connected_nodes, fn(node_name) {
        io.println("- " <> atom.to_string(node.to_atom(node_name)))
      })
    }
  }
}

// Print usage information
fn print_usage() -> Nil {
  io.println("Service Manager CLI")
  io.println("==================")
  io.println("\nUsage:")
  io.println("  service_manager start <service>     - Start a systemd service")
  io.println("  service_manager stop <service>      - Stop a systemd service")
  io.println(
    "  service_manager restart <service>   - Restart a systemd service",
  )
  io.println("  service_manager status <service>    - Check service status")
  io.println(
    "  service_manager list                - List all managed services",
  )
  io.println("  service_manager list-cluster        - List all connected nodes")
  io.println("\nSpecial commands:")
  io.println(
    "  service_manager --init-primary           - Start primary Khepri node",
  )
  io.println(
    "  service_manager --init-secondary <node>  - Start secondary Khepri node",
  )
}

// In src/service_manager.gleam - Updated monitor functions

// Function to monitor for new nodes joining the cluster
fn start_node_monitor() -> Nil {
  // Set up an actor to monitor nodes
  let _ = process.start(fn() { monitor_loop(list.new()) }, True)
  Nil
}

fn monitor_loop(known_nodes: List(String)) -> Nil {
  // Get current connected nodes
  let current_nodes =
    node.visible()
    |> list.map(fn(n) { atom.to_string(node.to_atom(n)) })

  // Find new nodes that weren't in our known list
  let new_nodes =
    list.filter(current_nodes, fn(node_name) {
      !list.contains(known_nodes, node_name)
    })

  // Print notification for each new node
  case list.length(new_nodes) {
    0 -> Nil
    _ -> {
      io.println("\n==== CLUSTER UPDATE ====")
      io.println(
        int.to_string(list.length(new_nodes))
        <> " new node(s) joined the cluster:",
      )

      list.each(new_nodes, fn(node_name) {
        io.println("➕ " <> node_name <> " has joined the cluster!")
      })

      // Wait much longer for replication to complete
      io.println("Waiting for data replication...")
      process.sleep(5000)
      // Increased wait time

      // Wait for leader election to complete on both nodes
      let _ = khepri_gleam_cluster.wait_for_leader(5000)

      // Automatic replication test - first check existing join notifications
      check_for_join_notifications()

      // Wait a bit more and try reading service states directly
      process.sleep(3000)
      // Increased wait time

      // Run more detailed replication verification
      io.println("\n==== REPLICATION VERIFICATION ====")
      io.println("Checking for all services in Khepri store:")

      case khepri_store.list_services() {
        Ok(services) -> {
          io.println(
            "Found " <> int.to_string(list.length(services)) <> " services",
          )
          list.each(services, fn(service) {
            let #(key, state) = service
            io.println("• " <> key <> ": " <> string.inspect(state))
          })

          // If we found services, replication is working
          case list.length(services) > 0 {
            True -> io.println("✅ Replication verification successful!")
            False ->
              io.println("⚠️ No services found - replication may not be working")
          }
        }
        Error(err) -> io.println("❌ Error checking services: " <> err)
      }

      io.println(
        "Total cluster size: "
        <> int.to_string(list.length(current_nodes) + 1)
        <> " nodes",
      )
      io.println("=====================\n")
    }
  }

  // Sleep for a bit, then check again with the updated known node list
  process.sleep(1000)
  monitor_loop(current_nodes)
}

fn check_for_join_notifications() -> Nil {
  // Wait longer for replication to complete
  process.sleep(3000)
  // Increased wait time

  io.println("Checking for join notifications...")

  // Use our new direct access function
  case khepri_store.list_services_direct() {
    Ok(services) -> {
      io.println(
        "Successfully retrieved "
        <> int.to_string(list.length(services))
        <> " services",
      )

      // Filter for join keys with better logging
      let join_messages =
        list.filter(services, fn(service) {
          let #(key, _) = service
          let is_join = string.contains(key, "join_")
          is_join
        })

      // Print the join notifications
      case list.length(join_messages) {
        0 -> io.println("No join notifications found")
        _ -> {
          io.println(
            "📝 Found "
            <> int.to_string(list.length(join_messages))
            <> " join notifications:",
          )
          list.each(join_messages, fn(message) {
            let #(key, state) = message
            io.println("  • " <> key <> ": " <> string.inspect(state))
          })
        }
      }
    }
    Error(err) -> {
      io.println("Error checking for join notifications: " <> err)
    }
  }
}

// Updated with better transaction handling
fn start_replication_test_cycle() -> Nil {
  // Start a background process that periodically tests replication
  let _ =
    process.start(
      fn() {
        // Wait a bit for initial setup
        process.sleep(5000)
        replication_test_loop(0)
      },
      True,
    )
  Nil
}

fn replication_test_loop(count: Int) -> Nil {
  // Run a replication test every 10 seconds
  io.println(
    "\n==== AUTO REPLICATION TEST #" <> int.to_string(count + 1) <> " ====",
  )

  // Wait for leader election to complete
  let _ = khepri_gleam_cluster.wait_for_leader(2000)

  // Write a test value
  let test_key = "auto_test_" <> int.to_string(count)
  let test_value = "Test value " <> int.to_string(count)

  case
    khepri_store.store_service_state(test_key, khepri_store.Custom(test_value))
  {
    Ok(_) -> io.println("✅ Test data written successfully: " <> test_key)
    Error(err) -> io.println("❌ Failed to write test data: " <> err)
  }

  // Wait longer for replication
  process.sleep(2000)
  // Increased wait time

  // Try to read back our own data
  case khepri_store.get_service_state(test_key) {
    Ok(_) -> io.println("✅ Successfully read back our own data")
    Error(err) -> io.println("❌ Failed to read our own data: " <> err)
  }

  io.println("=====================\n")

  // Sleep and continue testing
  process.sleep(10_000)
  replication_test_loop(count + 1)
}

// Enhanced continuous monitoring for the primary node
fn start_continuous_replication_monitor() -> Nil {
  // Start a background process that checks for replicated data
  let _ = process.start(fn() { continuous_replication_monitor(0) }, True)
  Nil
}

// In src/service_manager.gleam - fixed continuous_replication_monitor function

// In src/service_manager.gleam - Enhanced continuous replication monitor
fn continuous_replication_monitor(count: Int) -> Nil {
  // Log header with iteration count
  io.println(
    "\n==== REPLICATION MONITOR CHECK #" <> int.to_string(count + 1) <> " ====",
  )

  // First debug - print all paths from registry
  let all_paths = khepri_store.get_all_paths()
  io.println("All registered paths in Khepri: " <> string.inspect(all_paths))

  // Try direct reads of known services first
  io.println("\nTrying direct reads of specific paths:")
  let test_paths = [
    "/:services/primary_test_1745448820789", "/:services/auto_test_0",
    "/:services/join_service_cli_1745448819914@127.0.0.1",
  ]

  // Check each path directly
  list.each(test_paths, fn(path) {
    case khepri_gleam.get_string(path) {
      Ok(value) -> io.println("✅ Found: " <> path <> " = " <> value)
      Error(_) -> io.println("❌ Not found: " <> path)
    }
  })

  // Now try listing with our improved function
  io.println("\nListing all services with improved function:")
  case khepri_store.list_services_direct() {
    Ok(services) -> {
      io.println(
        "Found " <> int.to_string(list.length(services)) <> " services",
      )
      list.each(services, fn(service) {
        let #(key, state) = service
        io.println("• " <> key <> ": " <> string.inspect(state))
      })

      // Check replication status
      case list.length(services) > 0 {
        True -> io.println("✅ REPLICATION IS WORKING!")
        False -> io.println("⏳ Replication not yet detected")
      }
    }
    Error(err) -> io.println("Error listing services: " <> err)
  }

  io.println("=====================\n")

  // Wait and check again
  process.sleep(5000)
  continuous_replication_monitor(count + 1)
}

fn start_primary_write_test() -> Nil {
  // Start a background process for the test
  let _ =
    process.start(
      fn() {
        // Wait a bit for any secondary to connect
        process.sleep(5000)
        primary_write_test_loop(0)
      },
      True,
    )
  Nil
}

fn primary_write_test_loop(count: Int) -> Nil {
  // Generate a unique key for this test round
  let timestamp = erlang.system_time(erlang.Millisecond)
  let test_key = "primary_test_" <> int.to_string(timestamp)
  let test_value = "written_by_primary_" <> int.to_string(count)

  io.println(
    "\n==== PRIMARY WRITE TEST #" <> int.to_string(count + 1) <> " ====",
  )
  io.println("Writing test data from PRIMARY node")
  io.println("Key: " <> test_key)
  io.println("Value: " <> test_value)

  // Write directly to the path
  let path = khepri_gleam.to_khepri_path("/:services/" <> test_key)
  khepri_gleam.put(path, test_value)

  // Log all paths
  let all_paths = khepri_store.get_all_paths()
  io.println("Updated path registry: " <> string.inspect(all_paths))

  // Allow time for replication
  io.println("Waiting for replication to secondary...")
  process.sleep(5000)

  io.println("=====================\n")

  // Continue testing every 15 seconds
  process.sleep(10_000)
  primary_write_test_loop(count + 1)
}

// Add to src/service_manager.gleam - Secondary node check function

fn start_secondary_read_test() -> Nil {
  // Start a background process for the test
  let _ =
    process.start(
      fn() {
        // Wait for the cluster to stabilize
        process.sleep(5000)
        secondary_read_test_loop(0)
      },
      True,
    )
  Nil
}

fn secondary_read_test_loop(count: Int) -> Nil {
  io.println(
    "\n==== SECONDARY READ TEST #" <> int.to_string(count + 1) <> " ====",
  )
  io.println("Checking if secondary can read primary's data...")

  // Use our direct path access function to find primary test keys
  case khepri_store.list_services_direct() {
    Ok(services) -> {
      // Filter for items written by the primary
      let primary_writes =
        list.filter(services, fn(service) {
          let #(key, _) = service
          string.contains(key, "primary_test_")
        })

      // Report findings
      case list.length(primary_writes) {
        0 -> {
          io.println("❌ No data from primary node found")
          io.println("Data replication from primary to secondary NOT WORKING")

          // Print all keys we do see to help debug
          io.println("\nKeys visible to secondary:")
          list.each(services, fn(service) {
            let #(key, _) = service
            io.println("• " <> key)
          })

          // Try direct read of a known primary key if primary has written anything
          io.println("\nTrying direct read of primary data:")
          let test_key = "primary_test_1745445835462"
          // Known key from primary
          case khepri_gleam.get_string("/:services/" <> test_key) {
            Ok(value) -> {
              io.println("✅ Successfully read primary key directly: " <> value)
              io.println(
                "Inconsistency detected - direct read works but listing doesn't",
              )
            }
            Error(err) -> io.println("❌ Direct read failed: " <> err)
          }
        }
        _ -> {
          io.println(
            "✅ Found "
            <> int.to_string(list.length(primary_writes))
            <> " items from primary",
          )
          io.println("BIDIRECTIONAL REPLICATION IS WORKING!")

          // Show the items
          list.each(primary_writes, fn(item) {
            let #(key, state) = item
            io.println("• " <> key <> ": " <> string.inspect(state))
          })
        }
      }
    }
    Error(err) -> {
      io.println("❌ Error listing services: " <> err)
    }
  }

  io.println("=====================\n")

  // Continue testing
  process.sleep(7000)
  secondary_read_test_loop(count + 1)
}
