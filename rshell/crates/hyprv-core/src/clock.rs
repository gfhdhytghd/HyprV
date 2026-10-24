use futures_util::StreamExt;
use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::unix::AsyncFd;

#[derive(Clone, Debug)]
pub enum ClockEvent {
    Minute(i64),
    ClockChanged(i64),
    PrepareForSleep(bool),
}
pub fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
/// CLOCK_REALTIME absolute timer with CANCEL_ON_SET detects wall-clock jumps.
pub struct MinuteClock {
    fd: AsyncFd<OwnedFd>,
}
impl MinuteClock {
    pub fn new() -> io::Result<Self> {
        let raw = unsafe {
            libc::timerfd_create(libc::CLOCK_REALTIME, libc::TFD_NONBLOCK | libc::TFD_CLOEXEC)
        };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let clock = Self {
            fd: unsafe { AsyncFd::register(fd) }.map_err(io::Error::other)?,
        };
        clock.arm()?;
        Ok(clock)
    }
    fn arm(&self) -> io::Result<()> {
        let spec = libc::itimerspec {
            it_interval: libc::timespec {
                tv_sec: 60,
                tv_nsec: 0,
            },
            it_value: libc::timespec {
                tv_sec: (unix_seconds() / 60 + 1) * 60,
                tv_nsec: 0,
            },
        };
        let result = unsafe {
            libc::timerfd_settime(
                self.fd.get_ref().as_raw_fd(),
                libc::TFD_TIMER_ABSTIME | libc::TFD_TIMER_CANCEL_ON_SET,
                &spec,
                std::ptr::null_mut(),
            )
        };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    pub async fn next(&self) -> io::Result<ClockEvent> {
        loop {
            let mut ready = self.fd.readable().await?;
            let result = ready.try_io(|fd| {
                let mut expirations = 0u64;
                let n = unsafe {
                    libc::read(
                        fd.get_ref().as_raw_fd(),
                        (&mut expirations as *mut u64).cast(),
                        8,
                    )
                };
                if n < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(ClockEvent::Minute(unix_seconds()))
                }
            });
            match result {
                Ok(Err(e)) if e.raw_os_error() == Some(libc::ECANCELED) => {
                    self.arm()?;
                    return Ok(ClockEvent::ClockChanged(unix_seconds()));
                }
                Ok(result) => return result,
                Err(_) => continue,
            }
        }
    }
}
/// Signal-driven logind helper. Resume consumers should refresh time immediately.
pub async fn logind_sleep(
    sender: calloop::channel::Sender<ClockEvent>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::Connection::system().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    let mut signals = proxy.receive_signal("PrepareForSleep").await?;
    if *shutdown.borrow() {
        return Ok(());
    }
    loop {
        tokio::select! {
            signal = signals.next() => match signal {
                Some(signal) => { let (sleeping,): (bool,) = signal.body().deserialize()?; if sender.send(ClockEvent::PrepareForSleep(sleeping)).is_err() { return Ok(()); } },
                None => return Ok(()),
            },
            _ = shutdown.changed() => return Ok(()),
        }
    }
}

/// A clock without UI subscribers owns no timerfd and produces no minute wakeups.
pub async fn run_clock(
    topic: crate::store::Topic<i64>,
    sender: calloop::channel::Sender<ClockEvent>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> io::Result<()> {
    let mut demand = topic.demand();
    let mut clock = None;
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }
        if *demand.borrow_and_update() == 0 {
            clock = None;
        } else if clock.is_none() {
            clock = Some(MinuteClock::new()?);
            if sender.send(ClockEvent::Minute(unix_seconds())).is_err() {
                return Ok(());
            }
        }
        tokio::select! {
            result = async { clock.as_ref().unwrap().next().await }, if clock.is_some() => {
                if sender.send(result?).is_err() { return Ok(()); }
            },
            result = demand.changed() => if result.is_err() { return Ok(()); },
            _ = shutdown.changed() => return Ok(()),
        }
    }
}

/// Shared suspend/resume state, published once by core so service actors do not each
/// subscribe to logind. `resume_generation` increments on every `PrepareForSleep(false)`;
/// actors re-snapshot (positions, counters) when it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SleepState {
    pub sleeping: bool,
    pub resume_generation: u64,
}
pub struct SleepTopic;
impl crate::store::TopicKey for SleepTopic {
    type Value = SleepState;
}
/// Signal-driven publisher for [`SleepTopic`]; run once per process (the shell host).
pub async fn run_sleep_topic(
    topic: crate::store::Topic<SleepState>,
    connection: Option<zbus::Connection>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = match connection {
        Some(c) => c,
        None => zbus::Connection::system().await?,
    };
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await?;
    let mut signals = proxy.receive_signal("PrepareForSleep").await?;
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }
        tokio::select! {
            signal = signals.next() => match signal {
                Some(signal) => {
                    let (sleeping,): (bool,) = signal.body().deserialize()?;
                    let mut state = topic.snapshot().value;
                    if !sleeping {
                        state.resume_generation += 1;
                    }
                    state.sleeping = sleeping;
                    topic.publish(state);
                }
                None => return Ok(()),
            },
            _ = shutdown.changed() => return Ok(()),
        }
    }
}
