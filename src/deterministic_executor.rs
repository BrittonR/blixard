/// Deterministic executor for controlling task execution order
/// This is the HEART of deterministic simulation testing!

use std::collections::{VecDeque, BinaryHeap};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};
use std::cmp::Ordering;
use futures::FutureExt;

/// A task that can be executed
struct Task {
    id: u64,
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    waker: Waker,
}

/// Event that can wake a task
#[derive(Debug, Clone)]
enum WakeEvent {
    Immediate(u64),        // Task ID to wake immediately
    Timed(Instant, u64),   // Wake at specific time
}

impl PartialEq for WakeEvent {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (WakeEvent::Timed(t1, id1), WakeEvent::Timed(t2, id2)) => t1 == t2 && id1 == id2,
            (WakeEvent::Immediate(id1), WakeEvent::Immediate(id2)) => id1 == id2,
            _ => false,
        }
    }
}

impl Eq for WakeEvent {}

impl PartialOrd for WakeEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WakeEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // Immediate events have highest priority
            (WakeEvent::Immediate(_), WakeEvent::Timed(_, _)) => Ordering::Less,
            (WakeEvent::Timed(_, _), WakeEvent::Immediate(_)) => Ordering::Greater,
            
            // For timed events, earlier time = higher priority (reverse order for min-heap)
            (WakeEvent::Timed(t1, _), WakeEvent::Timed(t2, _)) => t2.cmp(t1),
            
            // For immediate events, use task ID for deterministic ordering
            (WakeEvent::Immediate(id1), WakeEvent::Immediate(id2)) => id2.cmp(id1),
        }
    }
}

/// Deterministic executor that controls task execution
pub struct DeterministicExecutor {
    tasks: Mutex<Vec<Option<Task>>>,
    wake_queue: Mutex<BinaryHeap<WakeEvent>>,
    current_time: Mutex<Instant>,
    next_task_id: Mutex<u64>,
    random_seed: u64,
}

impl DeterministicExecutor {
    pub fn new(seed: u64) -> Self {
        Self {
            tasks: Mutex::new(Vec::new()),
            wake_queue: Mutex::new(BinaryHeap::new()),
            current_time: Mutex::new(Instant::now()),
            next_task_id: Mutex::new(0),
            random_seed: seed,
        }
    }
    
    /// Spawn a new task
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) -> u64 {
        let task_id = {
            let mut next_id = self.next_task_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };
        
        let waker = self.create_waker(task_id);
        let task = Task {
            id: task_id,
            future: Box::pin(future),
            waker,
        };
        
        // Add task to list
        {
            let mut tasks = self.tasks.lock().unwrap();
            if task_id as usize >= tasks.len() {
                tasks.resize_with(task_id as usize + 1, || None);
            }
            tasks[task_id as usize] = Some(task);
        }
        
        // Wake it immediately
        self.wake_task(task_id);
        
        task_id
    }
    
    /// Create a waker for a task
    fn create_waker(&self, task_id: u64) -> Waker {
        struct TaskWaker {
            task_id: u64,
            executor: Arc<DeterministicExecutor>,
        }
        
        impl Wake for TaskWaker {
            fn wake(self: Arc<Self>) {
                self.executor.wake_task(self.task_id);
            }
        }
        
        Waker::from(Arc::new(TaskWaker {
            task_id,
            executor: Arc::new(unsafe { 
                // This is safe because we only use it within the executor
                std::ptr::read(self as *const _)
            }),
        }))
    }
    
    /// Wake a task
    fn wake_task(&self, task_id: u64) {
        let mut wake_queue = self.wake_queue.lock().unwrap();
        wake_queue.push(WakeEvent::Immediate(task_id));
    }
    
    /// Schedule a task to wake at a specific time
    pub fn wake_at(&self, time: Instant, task_id: u64) {
        let mut wake_queue = self.wake_queue.lock().unwrap();
        wake_queue.push(WakeEvent::Timed(time, task_id));
    }
    
    /// Get current virtual time
    pub fn current_time(&self) -> Instant {
        *self.current_time.lock().unwrap()
    }
    
    /// Advance virtual time
    pub fn advance_time(&self, duration: Duration) {
        let mut current = self.current_time.lock().unwrap();
        *current += duration;
    }
    
    /// Run until all tasks are blocked or complete
    pub fn run_until_idle(&self) {
        loop {
            // Get next wake event
            let next_event = {
                let mut wake_queue = self.wake_queue.lock().unwrap();
                wake_queue.pop()
            };
            
            let task_id = match next_event {
                Some(WakeEvent::Immediate(id)) => id,
                Some(WakeEvent::Timed(time, id)) => {
                    // Advance time if needed
                    let mut current = self.current_time.lock().unwrap();
                    if time > *current {
                        *current = time;
                    }
                    id
                }
                None => {
                    // No more events
                    break;
                }
            };
            
            // Get the task
            let mut task = {
                let mut tasks = self.tasks.lock().unwrap();
                if task_id as usize >= tasks.len() {
                    continue;
                }
                tasks[task_id as usize].take()
            };
            
            if let Some(mut task) = task {
                // Poll the task
                let waker = task.waker.clone();
                let mut cx = Context::from_waker(&waker);
                
                match task.future.as_mut().poll(&mut cx) {
                    Poll::Ready(()) => {
                        // Task complete, don't put it back
                    }
                    Poll::Pending => {
                        // Task not ready, put it back
                        let mut tasks = self.tasks.lock().unwrap();
                        tasks[task_id as usize] = Some(task);
                    }
                }
            }
        }
    }
    
    /// Run until a specific time
    pub fn run_until(&self, target_time: Instant) {
        while self.current_time() < target_time {
            // Check if we have any events before target time
            let has_events = {
                let wake_queue = self.wake_queue.lock().unwrap();
                wake_queue.iter().any(|event| match event {
                    WakeEvent::Immediate(_) => true,
                    WakeEvent::Timed(time, _) => *time <= target_time,
                })
            };
            
            if has_events {
                self.run_until_idle();
            } else {
                // Jump to target time
                let mut current = self.current_time.lock().unwrap();
                *current = target_time;
                break;
            }
        }
    }
}

/// A handle to spawn tasks on the executor
#[derive(Clone)]
pub struct ExecutorHandle {
    executor: Arc<DeterministicExecutor>,
}

impl ExecutorHandle {
    pub fn new(executor: Arc<DeterministicExecutor>) -> Self {
        Self { executor }
    }
    
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.executor.spawn(future);
    }
    
    pub fn current_time(&self) -> Instant {
        self.executor.current_time()
    }
    
    pub fn sleep(&self, duration: Duration) -> impl Future<Output = ()> {
        let executor = self.executor.clone();
        let target_time = executor.current_time() + duration;
        
        // Create a future that completes when time advances
        futures::future::poll_fn(move |cx| {
            if executor.current_time() >= target_time {
                Poll::Ready(())
            } else {
                // Schedule wake at target time
                // This is simplified - in real implementation we'd track the waker
                Poll::Pending
            }
        })
    }
}