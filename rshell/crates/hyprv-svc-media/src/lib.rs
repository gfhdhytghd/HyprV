//! Media-domain service actors for the HyprV shell: PipeWire audio and spectrum, MPRIS
//! (with the Cider compatibility adapter), StatusNotifier tray and SwayNC notifications.
//! Pure event driven: D-Bus signals and PipeWire events only; no subprocesses.
pub mod audio;
pub mod cider;
pub mod mpris;
pub mod notifications;
pub mod spectrum;
pub mod tray;
mod common;
pub use common::{monotonic_us, CounterSnapshot, Counters, Context, Error, Handle, PendingAction, Requester, Result};
