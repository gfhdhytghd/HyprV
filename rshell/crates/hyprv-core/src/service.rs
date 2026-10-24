use std::{future::Future, pin::Pin};
use tokio::sync::watch;
pub type ServiceResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
pub type ServiceFuture<'a> = Pin<Box<dyn Future<Output = ServiceResult> + Send + 'a>>;
/// Start/stop must be cancellation safe; stop must release subscriptions/timers.
pub trait Service: Send {
    fn start(&mut self) -> ServiceFuture<'_>;
    fn stop(&mut self) -> ServiceFuture<'_>;
}
/// Sampling actors are active only while the topic has subscribers. No idle timer.
/// A start failure terminates the lifecycle rather than silently retrying forever.
pub async fn run_sampled(
    mut service: impl Service,
    mut demand: watch::Receiver<usize>,
    mut shutdown: watch::Receiver<bool>,
) -> ServiceResult {
    let mut active = false;
    loop {
        let needed = *demand.borrow_and_update() > 0 && !*shutdown.borrow();
        if needed != active {
            if needed {
                service.start().await?;
            } else {
                service.stop().await?;
            }
            active = needed;
        }
        if *shutdown.borrow() {
            break;
        }
        tokio::select! {
            result = demand.changed() => if result.is_err() { break; },
            result = shutdown.changed() => if result.is_err() { break; },
        }
    }
    if active {
        service.stop().await?;
    }
    Ok(())
}

/// Combined demand (the `SamplingLease` of services-api.md): `1` while the topic has
/// subscribers **and** every gate is open, else `0`. Returns the receiver to pass to
/// [`run_sampled`] and the driver future to spawn next to it; the driver ends when any
/// input closes or `shutdown` fires. No timers.
pub fn gated_demand(
    mut demand: watch::Receiver<usize>,
    mut gates: Vec<watch::Receiver<bool>>,
    mut shutdown: watch::Receiver<bool>,
) -> (watch::Receiver<usize>, impl Future<Output = ()> + Send) {
    let value = |d: &mut watch::Receiver<usize>, g: &mut [watch::Receiver<bool>]| {
        // Mark every gate seen (no short-circuit) so none stays spuriously "changed".
        let mut open = true;
        for gate in g.iter_mut() {
            open &= *gate.borrow_and_update();
        }
        usize::from(*d.borrow_and_update() > 0 && open)
    };
    let (tx, rx) = watch::channel(value(&mut demand, &mut gates));
    let driver = async move {
        loop {
            let gate_changed = async {
                if gates.is_empty() {
                    std::future::pending::<Result<(), watch::error::RecvError>>().await
                } else {
                    futures_util::future::select_all(gates.iter_mut().map(|g| Box::pin(g.changed()))).await.0
                }
            };
            tokio::select! {
                r = demand.changed() => if r.is_err() { break },
                r = gate_changed => if r.is_err() { break },
                _ = shutdown.changed() => break,
                _ = tx.closed() => break,
            }
            let next = value(&mut demand, &mut gates);
            tx.send_if_modified(|v| std::mem::replace(v, next) != next);
        }
    };
    (rx, driver)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(tokio::sync::mpsc::UnboundedSender<&'static str>);
    impl Service for Fake {
        fn start(&mut self) -> ServiceFuture<'_> {
            Box::pin(async {
                self.0.send("start")?;
                Ok(())
            })
        }
        fn stop(&mut self) -> ServiceFuture<'_> {
            Box::pin(async {
                self.0.send("stop")?;
                Ok(())
            })
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn demand_transitions_and_shutdown() {
        let topic = crate::store::Topic::new(0);
        let (events, mut receive) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown, signal) = watch::channel(false);
        let task = tokio::spawn(run_sampled(Fake(events), topic.demand(), signal));
        tokio::task::yield_now().await;
        assert!(receive.try_recv().is_err());
        let first = topic.subscribe();
        assert_eq!(receive.recv().await.unwrap(), "start");
        let second = topic.subscribe();
        drop(first);
        tokio::task::yield_now().await;
        assert!(receive.try_recv().is_err());
        drop(second);
        assert_eq!(receive.recv().await.unwrap(), "stop");
        let third = topic.subscribe();
        assert_eq!(receive.recv().await.unwrap(), "start");
        shutdown.send_replace(true);
        assert_eq!(receive.recv().await.unwrap(), "stop");
        task.await.unwrap().unwrap();
        drop(third);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn gated_demand_requires_subscribers_and_gate() {
        let topic = crate::store::Topic::new(0);
        let (gate_tx, gate) = watch::channel(false);
        let (_stop, shutdown) = watch::channel(false);
        let (mut demand, driver) = gated_demand(topic.demand(), vec![gate], shutdown);
        tokio::spawn(driver);
        assert_eq!(*demand.borrow(), 0);
        let sub = topic.subscribe();
        tokio::task::yield_now().await;
        assert_eq!(*demand.borrow_and_update(), 0, "gate closed");
        gate_tx.send_replace(true);
        demand.changed().await.unwrap();
        assert_eq!(*demand.borrow_and_update(), 1);
        drop(sub);
        demand.changed().await.unwrap();
        assert_eq!(*demand.borrow_and_update(), 0);
    }
}
