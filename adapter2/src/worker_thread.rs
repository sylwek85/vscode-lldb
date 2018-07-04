use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Clone)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

pub struct CancellationSource {
    flag: Arc<AtomicBool>,
}

impl CancellationSource {
    pub fn new() -> Self {
        CancellationSource {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        CancellationToken {
            flag: self.flag.clone(),
        }
    }

    pub fn request_cancellation(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////

pub struct WorkerThread {
    join_handle: thread::JoinHandle<()>,
    cancel: CancellationSource,
}

impl WorkerThread {
    pub fn spawn<F>(f: F) -> Self
    where
        F: FnOnce(CancellationToken) + Send + 'static,
    {
        let cancel = CancellationSource::new();
        let token = cancel.cancellation_token();
        WorkerThread {
            join_handle: thread::spawn(move || f(token)),
            cancel: cancel,
        }
    }
}

impl Drop for WorkerThread {
    fn drop(&mut self) {
        self.cancel.request_cancellation();
    }
}
