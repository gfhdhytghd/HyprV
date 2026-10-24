//! Laptop's Quickshell scannerEnabled lease. Passive state observers never scan.
use crate::{network::State, Result};
use hyprv_core::{
    profile::Profile,
    store::{Subscription, Topic},
};
use std::time::Duration;
use tokio::{sync::watch, task::JoinHandle, time::Instant};

// Quickshell NMWirelessDevice::mScanIntervalMs (wireless.hpp).
pub(crate) const INTERVAL: Duration = Duration::from_millis(10001);

#[derive(Clone)]
pub struct ScanLease(Topic<()>);
impl Default for ScanLease {
    fn default() -> Self {
        Self(Topic::new(()))
    }
}
impl ScanLease {
    /// Hold only while a Wi-Fi page is requested, not during its closing animation.
    /// Multiple outputs share a scanner; dropping the final guard disarms it.
    pub fn acquire(&self) -> Subscription<()> {
        self.0.subscribe()
    }
}

pub(crate) struct Scanner {
    pub laptop: bool,
    pub readonly: bool,
    pub demand: watch::Receiver<usize>,
    pub deadline: Option<Instant>,
    pub job: Option<JoinHandle<Result<String>>>,
    last_request: Option<(String, Instant)>,
}
impl Scanner {
    pub fn new(profile: Profile, readonly: bool, lease: &ScanLease) -> Self {
        Self {
            laptop: profile == Profile::Laptop,
            readonly,
            demand: lease.0.demand(),
            deadline: None,
            job: None,
            last_request: None,
        }
    }
    pub fn visible(&self) -> bool {
        self.laptop && *self.demand.borrow() > 0
    }
    pub fn update(&mut self, state: &State, busy: bool, backend_delay: Duration) {
        if !self.visible()
            || self.readonly
            || !state.available
            || !state.present
            || !state.enabled
            || !state.hardware_enabled
            || state.device.is_none()
            || busy
        {
            self.cancel();
        } else if self.job.is_none() {
            let now = Instant::now();
            let requested = self
                .last_request
                .as_ref()
                .filter(|(d, _)| Some(d) == state.device.as_ref())
                .map_or(now, |(_, t)| *t + INTERVAL);
            self.deadline = Some(requested.max(now + backend_delay));
        }
    }
    pub fn requested(&mut self, device: Option<&str>) {
        self.last_request = device.map(|d| (d.to_owned(), Instant::now()));
        self.deadline = None;
    }
    pub fn reset(&mut self) {
        self.cancel();
        self.last_request = None;
    }
    fn cancel(&mut self) {
        self.deadline = None;
        if let Some(job) = self.job.take() {
            job.abort();
        }
    }
}
impl Drop for Scanner {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// NM LastScan is milliseconds since boot (including suspend), not Unix time.
pub(crate) fn backend_delay(last_scan: i64, boot_ms: i64) -> Duration {
    if last_scan < 0 {
        return Duration::ZERO;
    }
    let age = boot_ms.saturating_sub(last_scan).max(0) as u64;
    INTERVAL.saturating_sub(Duration::from_millis(age))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_scan_age_uses_boottime_and_limits_invalid_future_readings() {
        assert_eq!(backend_delay(-1, 100000), Duration::ZERO);
        assert_eq!(backend_delay(100, 10100), Duration::from_millis(1));
        assert_eq!(backend_delay(100, 10101), Duration::ZERO);
        assert_eq!(backend_delay(100, 99), INTERVAL);
        assert_eq!(backend_delay(i64::MAX, 0), INTERVAL);
    }
    #[tokio::test(start_paused = true)]
    async fn last_page_close_aborts_pending_scan_and_clears_deadline() {
        struct Cancelled(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Cancelled {
            fn drop(&mut self) {
                let _ = self.0.take().unwrap().send(());
            }
        }
        let lease = ScanLease::default();
        let guard = lease.acquire();
        let mut scanner = Scanner::new(Profile::Laptop, false, &lease);
        let (cancelled, done) = tokio::sync::oneshot::channel();
        let (started, entered) = tokio::sync::oneshot::channel();
        scanner.job = Some(tokio::spawn(async move {
            let _cancelled = Cancelled(Some(cancelled));
            started.send(()).unwrap();
            std::future::pending().await
        }));
        entered.await.unwrap();
        scanner.deadline = Some(Instant::now() + INTERVAL);
        drop(guard);
        scanner.update(&State::default(), false, Duration::ZERO);
        done.await.unwrap();
        assert!(scanner.job.is_none());
        assert!(scanner.deadline.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn hidden_disabled_and_missing_devices_have_no_deadline() {
        let lease = ScanLease::default();
        let mut scanner = Scanner::new(Profile::Laptop, false, &lease);
        let mut state = State {
            available: true,
            present: true,
            enabled: true,
            hardware_enabled: true,
            device: Some("wlan0".into()),
            ..State::default()
        };
        scanner.update(&state, false, Duration::ZERO);
        assert!(scanner.deadline.is_none());
        let guard = lease.acquire();
        scanner.update(&state, false, Duration::ZERO);
        assert_eq!(scanner.deadline, Some(Instant::now()));
        scanner.requested(state.device.as_deref());
        scanner.update(&state, false, Duration::ZERO);
        assert_eq!(scanner.deadline, Some(Instant::now() + INTERVAL));
        state.enabled = false;
        scanner.update(&state, false, Duration::ZERO);
        assert!(scanner.deadline.is_none());
        state.enabled = true;
        state.device = None;
        scanner.update(&state, false, Duration::ZERO);
        assert!(scanner.deadline.is_none());
        drop(guard);
        tokio::time::advance(Duration::from_secs(3600)).await;
        scanner.update(&state, false, Duration::ZERO);
        assert!(scanner.deadline.is_none());
    }
}
