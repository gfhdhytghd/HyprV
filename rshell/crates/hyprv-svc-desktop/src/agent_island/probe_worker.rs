//! One sleeping worker and one reusable handoff slot, rather than a boxed job
//! and a cloned root path for each leased discovery tick.
use super::model::ProcTable;
use crate::util::Proc;
use std::{
    path::Path,
    sync::{Arc, Condvar, Mutex},
};
#[derive(Default)]
struct Slot {
    request: Option<(ProcTable, f64)>,
    reply: Option<(ProcTable, bool)>,
    stop: bool,
}
struct Shared {
    slot: Mutex<Slot>,
    ready: Condvar,
    done: tokio::sync::Notify,
}
pub(super) struct ProbeWorker {
    shared: Arc<Shared>,
}
impl ProbeWorker {
    pub fn new(root: &Path) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot::default()),
            ready: Condvar::new(),
            done: tokio::sync::Notify::new(),
        });
        let worker = shared.clone();
        let proc = Proc {
            root: root.to_owned(),
        };
        std::thread::Builder::new()
            .name("hyprv-agentproc".into())
            .spawn(move || loop {
                let (mut table, now) = {
                    let mut slot = worker.slot.lock().unwrap();
                    while slot.request.is_none() && !slot.stop {
                        slot = worker.ready.wait(slot).unwrap();
                    }
                    if slot.stop {
                        return;
                    }
                    slot.request.take().unwrap()
                };
                let changed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    table.refresh(&proc, now)
                }))
                .unwrap_or(true);
                {
                    let mut slot = worker.slot.lock().unwrap();
                    if slot.stop {
                        return;
                    }
                    slot.reply = Some((table, changed));
                }
                worker.done.notify_one();
            })?;
        Ok(Self { shared })
    }
    pub async fn probe(&mut self, table: &mut ProcTable, now: f64) -> bool {
        {
            let mut slot = self.shared.slot.lock().unwrap();
            debug_assert!(slot.request.is_none() && slot.reply.is_none());
            slot.request = Some((std::mem::take(table), now));
        }
        self.shared.ready.notify_one();
        loop {
            self.shared.done.notified().await;
            if let Some((next, changed)) = self.shared.slot.lock().unwrap().reply.take() {
                *table = next;
                return changed;
            }
        }
    }
}
impl Drop for ProbeWorker {
    fn drop(&mut self) {
        self.shared.slot.lock().unwrap().stop = true;
        self.shared.ready.notify_one();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn completion_and_drop_release_worker_and_cached_fds() {
        let mut worker = ProbeWorker::new(Path::new("/proc")).unwrap();
        let weak = Arc::downgrade(&worker.shared);
        let mut table = ProcTable::default();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            worker.probe(&mut table, 1.),
        )
        .await
        .unwrap();
        assert!(!table.procs.is_empty());
        drop(worker);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while weak.upgrade().is_some() {
                tokio::task::yield_now().await
            }
        })
        .await
        .unwrap();
    }
}
