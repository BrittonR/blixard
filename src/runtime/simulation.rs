use crate::runtime_traits::*;
use std::collections::{HashMap, VecDeque, BTreeMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, Waker, Wake};
use std::sync::Weak;
use std::time::{Duration, Instant};
use std::net::SocketAddr;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::future::Future;
use futures::future::BoxFuture;
use futures::stream::{Stream, StreamExt};
use parking_lot::RwLock;
use async_trait::async_trait;

/// Deterministic runtime for simulation testing
pub struct SimulatedRuntime {
    inner: Arc<SimulatedRuntimeInner>,
}

struct SimulatedRuntimeInner {
    clock: SimulatedClock,
    random: Mutex<SimulatedRandom>,
    file_system: SimulatedFileSystem,
    network: SimulatedNetwork,
    executor: Arc<Mutex<DeterministicExecutor>>,
    sleep_wakers: Arc<Mutex<BTreeMap<Instant, Vec<Waker>>>>,
}

impl SimulatedRuntime {
    /// Create a new simulated runtime with a seed for reproducibility
    pub fn new(seed: u64) -> Self {
        let sleep_wakers = Arc::new(Mutex::new(BTreeMap::new()));
        let executor = Arc::new(Mutex::new(DeterministicExecutor::new()));
        
        // Set the self reference in the executor
        executor.lock().unwrap().set_self_ref(executor.clone());
        
        let inner = Arc::new(SimulatedRuntimeInner {
            clock: SimulatedClock::new(seed, sleep_wakers.clone()),
            random: Mutex::new(SimulatedRandom::new(seed)),
            file_system: SimulatedFileSystem::new(),
            network: SimulatedNetwork::new(),
            executor,
            sleep_wakers,
        });
        
        Self { inner }
    }
    
    /// Spawn a future on the deterministic executor
    pub fn spawn_on_executor(&self, future: BoxFuture<'static, ()>) {
        self.inner.executor.lock().unwrap().spawn(future);
    }
    
    /// Advance virtual time by the given duration
    pub fn advance_time(&self, duration: Duration) {
        self.inner.clock.advance(duration);
        
        // Wake up any sleep futures that have expired
        let now = self.inner.clock.now();
        let mut wakers_to_wake = Vec::new();
        {
            let mut sleep_wakers = self.inner.sleep_wakers.lock().unwrap();
            let expired: Vec<_> = sleep_wakers
                .range(..=now)
                .map(|(k, _)| *k)
                .collect();
            
            for instant in expired {
                if let Some(wakers) = sleep_wakers.remove(&instant) {
                    wakers_to_wake.extend(wakers);
                }
            }
        }
        
        // Wake all expired timers
        for waker in wakers_to_wake {
            waker.wake();
        }
        
        self.inner.executor.lock().unwrap().process_timers(now);
    }
    
    /// Run the simulation until all tasks are complete or blocked
    pub fn run_until_idle(&self) {
        let mut executor = self.inner.executor.lock().unwrap();
        executor.run_until_idle();
    }
    
    /// Inject a network partition between two addresses
    pub fn partition_network(&self, addr1: SocketAddr, addr2: SocketAddr) {
        self.inner.network.add_partition(addr1, addr2);
    }
    
    /// Heal a network partition
    pub fn heal_network(&self, addr1: SocketAddr, addr2: SocketAddr) {
        self.inner.network.remove_partition(addr1, addr2);
    }
    
    /// Set network latency between two nodes
    pub fn set_network_latency(&self, from: SocketAddr, to: SocketAddr, latency: Duration) {
        self.inner.network.set_latency(from, to, latency);
    }
}

/// Deterministic clock for simulation
pub struct SimulatedClock {
    current_time: Arc<AtomicU64>,
    base_instant: Instant,
    sleep_wakers: Arc<Mutex<BTreeMap<Instant, Vec<Waker>>>>,
}

impl SimulatedClock {
    fn new(seed: u64, sleep_wakers: Arc<Mutex<BTreeMap<Instant, Vec<Waker>>>>) -> Self {
        // Create a deterministic base instant by using a fixed offset from the Unix epoch
        // We simulate the base as if it's Jan 1, 2020 + some offset based on seed
        // This gives us reproducible start times
        let base_nanos = 1_577_836_800_000_000_000u64; // Jan 1, 2020 in nanoseconds since epoch
        let seed_offset_nanos = (seed % 1_000_000) * 1_000_000; // Small offset based on seed
        
        // Create base instant that appears to be in the past but deterministic
        // Note: We can't make Instant truly deterministic across different process runs,
        // but we can make the relative times deterministic
        let now = Instant::now();
        let base_instant = now - Duration::from_secs(1_000_000); // Fixed offset
        
        // Start time based on seed - this gives us deterministic elapsed times
        let initial_nanos = base_nanos + seed_offset_nanos;
        
        Self {
            current_time: Arc::new(AtomicU64::new(initial_nanos)),
            base_instant,
            sleep_wakers,
        }
    }
    
    fn advance(&self, duration: Duration) {
        self.current_time.fetch_add(duration.as_nanos() as u64, Ordering::SeqCst);
    }
    
    fn current_duration(&self) -> Duration {
        Duration::from_nanos(self.current_time.load(Ordering::SeqCst))
    }
}

impl Clock for SimulatedClock {
    fn now(&self) -> Instant {
        self.base_instant + self.current_duration()
    }
    
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let target_time = self.now() + duration;
        Box::pin(SimulatedSleep {
            target_time,
            clock: self,
        })
    }
    
    fn timer(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let target_time = self.now() + duration;
        Box::pin(SimulatedSleep {
            target_time,
            clock: self,
        })
    }
    
    fn interval(&self, duration: Duration) -> Pin<Box<dyn Stream<Item = ()> + Send + '_>> {
        Box::pin(futures::stream::unfold(
            (self, duration, self.now()),
            |(clock, duration, mut next_tick)| async move {
                clock.sleep(next_tick.duration_since(clock.now())).await;
                next_tick = next_tick + duration;
                Some(((), (clock, duration, next_tick)))
            }
        ))
    }
}

struct SimulatedSleep<'a> {
    target_time: Instant,
    clock: &'a SimulatedClock,
}

impl<'a> Future for SimulatedSleep<'a> {
    type Output = ();
    
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.clock.now() >= self.target_time {
            Poll::Ready(())
        } else {
            // Register waker to be called when time advances past target
            let mut sleep_wakers = self.clock.sleep_wakers.lock().unwrap();
            sleep_wakers
                .entry(self.target_time)
                .or_insert_with(Vec::new)
                .push(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// Deterministic random number generator
pub struct SimulatedRandom {
    seed: u64,
    state: u64,
}

impl SimulatedRandom {
    fn new(seed: u64) -> Self {
        Self { seed, state: seed }
    }
    
    /// Linear congruential generator for deterministic randomness
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
}

impl Random for SimulatedRandom {
    fn next_u64(&mut self) -> u64 {
        self.next()
    }
}

/// Simulated filesystem that stores everything in memory
pub struct SimulatedFileSystem {
    files: Arc<RwLock<HashMap<std::path::PathBuf, Vec<u8>>>>,
}

impl SimulatedFileSystem {
    fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl FileSystem for SimulatedFileSystem {
    async fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .read()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))
    }
    
    async fn write(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        self.files.write().insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }
    
    async fn create_dir(&self, _path: &Path) -> io::Result<()> {
        // In simulation, directories are implicit
        Ok(())
    }
    
    async fn exists(&self, path: &Path) -> bool {
        self.files.read().contains_key(path)
    }
    
    async fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.files
            .write()
            .remove(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not found"))?;
        Ok(())
    }
}

/// Simulated network with controllable failures and latency
pub struct SimulatedNetwork {
    inner: Arc<SimulatedNetworkInner>,
}

struct SimulatedNetworkInner {
    partitions: RwLock<HashSet<(SocketAddr, SocketAddr)>>,
    latencies: RwLock<HashMap<(SocketAddr, SocketAddr), Duration>>,
    connections: RwLock<HashMap<u64, SimulatedConnection>>,
    next_conn_id: AtomicU64,
}

impl SimulatedNetwork {
    fn new() -> Self {
        Self {
            inner: Arc::new(SimulatedNetworkInner {
                partitions: RwLock::new(HashSet::new()),
                latencies: RwLock::new(HashMap::new()),
                connections: RwLock::new(HashMap::new()),
                next_conn_id: AtomicU64::new(0),
            }),
        }
    }
    
    fn add_partition(&self, addr1: SocketAddr, addr2: SocketAddr) {
        let mut partitions = self.inner.partitions.write();
        partitions.insert((addr1, addr2));
        partitions.insert((addr2, addr1));
    }
    
    fn remove_partition(&self, addr1: SocketAddr, addr2: SocketAddr) {
        let mut partitions = self.inner.partitions.write();
        partitions.remove(&(addr1, addr2));
        partitions.remove(&(addr2, addr1));
    }
    
    fn set_latency(&self, from: SocketAddr, to: SocketAddr, latency: Duration) {
        self.inner.latencies.write().insert((from, to), latency);
    }
    
    fn is_partitioned(&self, from: SocketAddr, to: SocketAddr) -> bool {
        self.inner.partitions.read().contains(&(from, to))
    }
    
    fn get_latency(&self, from: SocketAddr, to: SocketAddr) -> Duration {
        self.inner.latencies
            .read()
            .get(&(from, to))
            .copied()
            .unwrap_or(Duration::from_millis(1))
    }
}

#[derive(Clone)]
pub struct SimulatedConnection {
    id: u64,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    incoming: Arc<Mutex<VecDeque<Vec<u8>>>>,
    network: Arc<SimulatedNetworkInner>,
}

#[async_trait]
impl Network for SimulatedNetwork {
    type Connection = SimulatedConnection;
    
    async fn bind(&self, addr: SocketAddr) -> io::Result<Self::Connection> {
        let id = self.inner.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let conn = SimulatedConnection {
            id,
            local_addr: addr,
            remote_addr: addr, // Will be set on accept
            incoming: Arc::new(Mutex::new(VecDeque::new())),
            network: Arc::clone(&self.inner),
        };
        self.inner.connections.write().insert(id, conn.clone());
        Ok(conn)
    }
    
    async fn connect(&self, addr: SocketAddr) -> io::Result<Self::Connection> {
        let id = self.inner.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let local_addr = SocketAddr::from(([127, 0, 0, 1], 10000 + id as u16));
        let conn = SimulatedConnection {
            id,
            local_addr,
            remote_addr: addr,
            incoming: Arc::new(Mutex::new(VecDeque::new())),
            network: Arc::clone(&self.inner),
        };
        self.inner.connections.write().insert(id, conn.clone());
        Ok(conn)
    }
    
    async fn send(&self, conn: &mut Self::Connection, data: &[u8]) -> io::Result<()> {
        if self.is_partitioned(conn.local_addr, conn.remote_addr) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "network partition"));
        }
        
        // Simulate latency
        let latency = self.get_latency(conn.local_addr, conn.remote_addr);
        tokio::time::sleep(latency).await;
        
        // Find the remote connection and deliver the data
        let connections = self.inner.connections.read();
        for (_, remote_conn) in connections.iter() {
            if remote_conn.local_addr == conn.remote_addr {
                remote_conn.incoming.lock().unwrap().push_back(data.to_vec());
                break;
            }
        }
        
        Ok(())
    }
    
    async fn recv(&self, conn: &mut Self::Connection, buf: &mut [u8]) -> io::Result<usize> {
        // Check for incoming data
        let mut incoming = conn.incoming.lock().unwrap();
        if let Some(data) = incoming.pop_front() {
            let len = data.len().min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            Ok(len)
        } else {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "no data available"))
        }
    }
}

/// Task handle for the deterministic executor
struct TaskHandle {
    executor: Weak<Mutex<DeterministicExecutor>>,
    task: Mutex<Option<BoxFuture<'static, ()>>>,
}

impl Wake for TaskHandle {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref()
    }
    
    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(executor) = self.executor.upgrade() {
            if let Some(task) = self.task.lock().unwrap().take() {
                executor.lock().unwrap().tasks.push_back(task);
            }
        }
    }
}

/// Deterministic executor for running futures
pub struct DeterministicExecutor {
    tasks: VecDeque<BoxFuture<'static, ()>>,
    timers: BTreeMap<Instant, Vec<Waker>>,
    self_ref: Option<Arc<Mutex<DeterministicExecutor>>>,
}

impl DeterministicExecutor {
    pub fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
            timers: BTreeMap::new(),
            self_ref: None,
        }
    }
    
    pub fn set_self_ref(&mut self, self_ref: Arc<Mutex<DeterministicExecutor>>) {
        self.self_ref = Some(self_ref);
    }
    
    pub fn spawn(&mut self, future: BoxFuture<'static, ()>) {
        self.tasks.push_back(future);
    }
    
    pub fn run_until_idle(&mut self) {
        let self_ref = self.self_ref.as_ref().expect("Executor self reference not set").clone();
        
        while let Some(mut task) = self.tasks.pop_front() {
            let task_handle = Arc::new(TaskHandle {
                executor: Arc::downgrade(&self_ref),
                task: Mutex::new(None),
            });
            let waker = Waker::from(task_handle.clone());
            let mut cx = Context::from_waker(&waker);
            
            match task.as_mut().poll(&mut cx) {
                Poll::Ready(()) => {}
                Poll::Pending => {
                    // Store the task in the handle so it can be re-queued when woken
                    *task_handle.task.lock().unwrap() = Some(task);
                }
            }
        }
    }
    
    pub fn process_timers(&mut self, now: Instant) {
        let expired_timers: Vec<_> = self.timers
            .range(..=now)
            .map(|(instant, _)| *instant)
            .collect();
            
        for instant in expired_timers {
            if let Some(wakers) = self.timers.remove(&instant) {
                for waker in wakers {
                    waker.wake();
                }
            }
        }
    }
}

impl Runtime for SimulatedRuntime {
    type Clock = SimulatedClock;
    type Random = SimulatedRandom;
    type FileSystem = SimulatedFileSystem;
    type Network = SimulatedNetwork;
    
    fn clock(&self) -> &Self::Clock {
        &self.inner.clock
    }
    
    fn random(&self) -> &Self::Random {
        // This is a hack for now - in real implementation we'd need a better design
        // that allows mutable access to the random number generator
        unimplemented!("Random access needs redesign for mutability")
    }
    
    fn file_system(&self) -> &Self::FileSystem {
        &self.inner.file_system
    }
    
    fn network(&self) -> &Self::Network {
        &self.inner.network
    }
    
    fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        // For now, use tokio's spawn but in a full implementation
        // we would use our deterministic executor
        tokio::spawn(future)
    }
}

