use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};

/// Trait for abstracting time operations for deterministic testing
pub trait Clock: Send + Sync {
    /// Get the current instant
    fn now(&self) -> Instant;
    
    /// Sleep for a duration
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    
    /// Create a timer that fires after a duration
    fn timer(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    
    /// Create an interval that fires every duration
    fn interval(&self, duration: Duration) -> Pin<Box<dyn Stream<Item = ()> + Send + '_>>;
}

/// Trait for abstracting random number generation
pub trait Random: Send + Sync {
    /// Generate a random u64
    fn next_u64(&mut self) -> u64;
    
    /// Generate a random value in range [0, max)
    fn gen_range(&mut self, max: u64) -> u64 {
        self.next_u64() % max
    }
    
    /// Generate a random boolean with given probability
    fn gen_bool(&mut self, probability: f64) -> bool {
        (self.next_u64() as f64 / u64::MAX as f64) < probability
    }
}

/// Trait for abstracting filesystem operations
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// Read a file
    async fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    
    /// Write a file
    async fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    
    /// Create a directory
    async fn create_dir(&self, path: &Path) -> io::Result<()>;
    
    /// Check if path exists
    async fn exists(&self, path: &Path) -> bool;
    
    /// Remove a file
    async fn remove_file(&self, path: &Path) -> io::Result<()>;
}

/// Trait for abstracting network operations
#[async_trait]
pub trait Network: Send + Sync {
    /// Type of network connections
    type Connection: Send + Sync;
    
    /// Bind to an address
    async fn bind(&self, addr: SocketAddr) -> io::Result<Self::Connection>;
    
    /// Connect to an address
    async fn connect(&self, addr: SocketAddr) -> io::Result<Self::Connection>;
    
    /// Send data on a connection
    async fn send(&self, conn: &mut Self::Connection, data: &[u8]) -> io::Result<()>;
    
    /// Receive data on a connection
    async fn recv(&self, conn: &mut Self::Connection, buf: &mut [u8]) -> io::Result<usize>;
}

/// Main runtime trait that combines all abstractions
pub trait Runtime: Send + Sync {
    type Clock: Clock;
    type Random: Random;
    type FileSystem: FileSystem;
    type Network: Network;
    
    /// Get the clock implementation
    fn clock(&self) -> &Self::Clock;
    
    /// Get the random number generator
    fn random(&self) -> &Self::Random;
    
    /// Get the filesystem implementation
    fn file_system(&self) -> &Self::FileSystem;
    
    /// Get the network implementation
    fn network(&self) -> &Self::Network;
    
    /// Spawn a future on the runtime
    fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;
}

/// Real runtime implementation for production use
pub struct RealRuntime {
    clock: RealClock,
    random: RealRandom,
    file_system: RealFileSystem,
    network: RealNetwork,
}

impl RealRuntime {
    pub fn new() -> Self {
        Self {
            clock: RealClock,
            random: RealRandom::new(),
            file_system: RealFileSystem,
            network: RealNetwork,
        }
    }
}

/// Real clock implementation
pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
    
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep(duration))
    }
    
    fn timer(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep(duration))
    }
    
    fn interval(&self, duration: Duration) -> Pin<Box<dyn Stream<Item = ()> + Send + '_>> {
        Box::pin(tokio_stream::wrappers::IntervalStream::new(
            tokio::time::interval(duration)
        ).map(|_| ()))
    }
}

/// Real random implementation
pub struct RealRandom {
    rng: rand::rngs::StdRng,
}

impl RealRandom {
    fn new() -> Self {
        use rand::SeedableRng;
        Self {
            rng: rand::rngs::StdRng::from_entropy(),
        }
    }
}

impl Random for RealRandom {
    fn next_u64(&mut self) -> u64 {
        use rand::Rng;
        self.rng.gen()
    }
}

/// Real filesystem implementation
pub struct RealFileSystem;

#[async_trait]
impl FileSystem for RealFileSystem {
    async fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        tokio::fs::read(path).await
    }
    
    async fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        tokio::fs::write(path, contents).await
    }
    
    async fn create_dir(&self, path: &Path) -> io::Result<()> {
        tokio::fs::create_dir_all(path).await
    }
    
    async fn exists(&self, path: &Path) -> bool {
        tokio::fs::metadata(path).await.is_ok()
    }
    
    async fn remove_file(&self, path: &Path) -> io::Result<()> {
        tokio::fs::remove_file(path).await
    }
}

/// Real network implementation
pub struct RealNetwork;

pub struct RealConnection {
    socket: tokio::net::TcpStream,
}

#[async_trait]
impl Network for RealNetwork {
    type Connection = RealConnection;
    
    async fn bind(&self, addr: SocketAddr) -> io::Result<Self::Connection> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let (socket, _) = listener.accept().await?;
        Ok(RealConnection { socket })
    }
    
    async fn connect(&self, addr: SocketAddr) -> io::Result<Self::Connection> {
        let socket = tokio::net::TcpStream::connect(addr).await?;
        Ok(RealConnection { socket })
    }
    
    async fn send(&self, conn: &mut Self::Connection, data: &[u8]) -> io::Result<()> {
        use tokio::io::AsyncWriteExt;
        conn.socket.write_all(data).await
    }
    
    async fn recv(&self, conn: &mut Self::Connection, buf: &mut [u8]) -> io::Result<usize> {
        use tokio::io::AsyncReadExt;
        conn.socket.read(buf).await
    }
}

impl Runtime for RealRuntime {
    type Clock = RealClock;
    type Random = RealRandom;
    type FileSystem = RealFileSystem;
    type Network = RealNetwork;
    
    fn clock(&self) -> &Self::Clock {
        &self.clock
    }
    
    fn random(&self) -> &Self::Random {
        &self.random
    }
    
    fn file_system(&self) -> &Self::FileSystem {
        &self.file_system
    }
    
    fn network(&self) -> &Self::Network {
        &self.network
    }
    
    fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(future)
    }
}

