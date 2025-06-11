use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
/// Global runtime context that allows switching between real and simulated runtime
/// This is the KEY to making deterministic testing work!
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::runtime::simulation::SimulatedRuntime;
use crate::runtime_traits::{Clock, Runtime};

// Thread-local storage for current runtime
thread_local! {
    static CURRENT_RUNTIME: RefCell<Option<Arc<dyn RuntimeHandle>>> = RefCell::new(None);
}

// Global default runtime (real by default)
static GLOBAL_RUNTIME: Lazy<RwLock<Arc<dyn RuntimeHandle>>> =
    Lazy::new(|| RwLock::new(Arc::new(RealRuntimeHandle)));

/// Abstract task handle that works for both real and simulated runtime
pub struct TaskHandle {
    // In simulation, we don't need actual handles
    _private: (),
}

/// Handle to interact with the current runtime
pub trait RuntimeHandle: Send + Sync {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
    fn now(&self) -> Instant;
}

/// Real runtime handle (uses tokio)
struct RealRuntimeHandle;

impl RuntimeHandle for RealRuntimeHandle {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        tokio::spawn(future);
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(tokio::time::sleep(duration))
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Simulated runtime handle
pub struct SimulatedRuntimeHandle {
    runtime: Arc<SimulatedRuntime>,
}

impl SimulatedRuntimeHandle {
    pub fn new(seed: u64) -> Self {
        Self {
            runtime: Arc::new(SimulatedRuntime::new(seed)),
        }
    }

    pub fn new_with_runtime(runtime: Arc<SimulatedRuntime>) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> &Arc<SimulatedRuntime> {
        &self.runtime
    }

    /// Run the deterministic executor until all tasks are idle
    pub fn run_until_idle(&self) {
        self.runtime.run_until_idle();
    }
}

impl RuntimeHandle for SimulatedRuntimeHandle {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        // Use tokio spawn but wrap the future to ensure it uses simulated time
        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            // Set thread-local runtime for this task
            let _guard = RuntimeGuard::new(
                Arc::new(SimulatedRuntimeHandle { runtime }) as Arc<dyn RuntimeHandle>
            );
            future.await
        });
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // For simulation, we need to make sleep instant when time is advanced
        let runtime = self.runtime.clone();
        let target_time = runtime.clock().now() + duration;

        Box::pin(async move {
            loop {
                let now = runtime.clock().now();
                if now >= target_time {
                    break;
                }
                // Yield to allow time advancement
                tokio::task::yield_now().await;
            }
        })
    }

    fn now(&self) -> Instant {
        self.runtime.clock().now()
    }
}

/// Set the global runtime (affects all new operations)
pub fn set_global_runtime(runtime: Arc<dyn RuntimeHandle>) {
    *GLOBAL_RUNTIME.write() = runtime;
}

/// Set thread-local runtime (overrides global for this thread)
pub fn set_thread_runtime(runtime: Arc<dyn RuntimeHandle>) {
    CURRENT_RUNTIME.with(|r| {
        *r.borrow_mut() = Some(runtime);
    });
}

/// Clear thread-local runtime (reverts to global)
pub fn clear_thread_runtime() {
    CURRENT_RUNTIME.with(|r| {
        *r.borrow_mut() = None;
    });
}

/// Get the current runtime handle
fn current_runtime() -> Arc<dyn RuntimeHandle> {
    CURRENT_RUNTIME.with(|r| {
        r.borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| GLOBAL_RUNTIME.read().clone())
    })
}

/// Spawn a future on the current runtime
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    current_runtime().spawn(Box::pin(future))
}

/// Sleep for a duration using the current runtime
pub async fn sleep(duration: Duration) {
    current_runtime().sleep(duration).await
}

/// Get current time from the runtime
pub fn now() -> Instant {
    current_runtime().now()
}

/// RAII guard to temporarily use a different runtime
pub struct RuntimeGuard {
    // We store this just to ensure it's dropped
    _phantom: std::marker::PhantomData<()>,
}

impl RuntimeGuard {
    pub fn new(runtime: Arc<dyn RuntimeHandle>) -> Self {
        set_thread_runtime(runtime);
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        clear_thread_runtime();
    }
}

/// Use a simulated runtime for the duration of a test (async version)
pub async fn with_simulated_runtime<F, Fut>(seed: u64, f: F) -> Fut::Output
where
    F: FnOnce(Arc<SimulatedRuntimeHandle>) -> Fut,
    Fut: Future,
{
    let sim_handle = Arc::new(SimulatedRuntimeHandle::new(seed));
    let _guard = RuntimeGuard::new(sim_handle.clone() as Arc<dyn RuntimeHandle>);
    let result = f(sim_handle.clone()).await;
    // Run any remaining tasks
    sim_handle.run_until_idle();
    result
}
