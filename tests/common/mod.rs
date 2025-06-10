use std::net::{TcpListener, SocketAddr};

/// Get an available port by binding to port 0
pub fn get_available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to port 0");
    let port = listener.local_addr()
        .expect("Failed to get local address")
        .port();
    drop(listener); // Release the port
    port
}

/// Get multiple available ports
pub fn get_available_ports(count: usize) -> Vec<u16> {
    (0..count)
        .map(|_| get_available_port())
        .collect()
}

/// Create a SocketAddr with an available port
pub fn get_test_addr() -> SocketAddr {
    format!("127.0.0.1:{}", get_available_port())
        .parse()
        .expect("Failed to parse address")
}