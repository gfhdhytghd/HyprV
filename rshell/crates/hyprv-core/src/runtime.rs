use std::{future::Future, io, thread};
use tokio::sync::{oneshot, watch};
/// Tokio owns service IO on one thread. calloop's channel wakes the UI only on send.
pub struct ServiceRuntime {
    handle: tokio::runtime::Handle,
    shutdown: watch::Sender<bool>,
    done: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
    thread_id: thread::ThreadId,
}
impl ServiceRuntime {
    pub fn new() -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let handle = runtime.handle().clone();
        let (shutdown, _) = watch::channel(false);
        let (done, wait) = oneshot::channel();
        let thread = thread::Builder::new()
            .name("hyprv-services".into())
            .spawn(move || {
                runtime.block_on(async {
                    let _ = wait.await;
                });
            })?;
        Ok(Self {
            handle,
            shutdown,
            done: Some(done),
            thread_id: thread.thread().id(),
            thread: Some(thread),
        })
    }
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn(future)
    }
    /// Run synchronous setup that needs a Tokio reactor (for example bind).
    pub fn enter<R>(&self, setup: impl FnOnce() -> R) -> R {
        let _guard = self.handle.enter();
        setup()
    }
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.handle.block_on(future)
    }
    pub fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }
    pub fn bridge<T: Send + 'static>() -> (calloop::channel::Sender<T>, calloop::channel::Channel<T>)
    {
        calloop::channel::channel()
    }
    pub fn request_shutdown(&self) {
        self.shutdown.send_replace(true);
    }
}
impl ServiceRuntime {
    /// Explicit controller teardown: signal shutdown, let actors finish, join the thread.
    /// Call from the UI/controller thread, never from a calloop callback that must not block.
    pub fn shutdown_and_join(mut self) {
        self.stop(true);
    }
    fn stop(&mut self, join: bool) {
        self.request_shutdown();
        self.done.take();
        if let Some(thread) = self.thread.take() {
            // The last owner may be released by an actor on the runtime thread itself
            // (CORE-1): joining there would self-deadlock. The thread then exits on its own
            // once `block_on` observes the dropped completion sender.
            if join && thread::current().id() != self.thread_id {
                let _ = thread.join();
            }
        }
    }
}
impl Drop for ServiceRuntime {
    fn drop(&mut self) {
        self.stop(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn last_owner_released_on_worker_thread_does_not_self_join() {
        let runtime = Arc::new(ServiceRuntime::new().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        let owner = runtime.clone();
        let mut shutdown = runtime.shutdown_signal();
        runtime.spawn(async move {
            let _ = shutdown.changed().await;
            // Release the final Arc on the runtime thread.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(owner)));
            tx.send(result.is_ok()).unwrap();
        });
        runtime.request_shutdown();
        drop(runtime); // not the last owner: must not wait for the actor's copy forever
        assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), "drop on worker panicked");
    }
    #[test]
    fn explicit_shutdown_and_join() {
        let runtime = ServiceRuntime::new().unwrap();
        let mut signal = runtime.shutdown_signal();
        let task = runtime.spawn(async move { let _ = signal.changed().await; 7 });
        runtime.request_shutdown();
        assert_eq!(runtime.block_on(task).unwrap(), 7);
        runtime.shutdown_and_join();
    }
}
