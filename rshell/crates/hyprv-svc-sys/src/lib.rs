//! Native system integrations. No subprocesses; one actor per module and store.
pub mod calendar;
pub mod hyprland;
pub mod mihomo;
mod support;
pub mod systemd;
pub mod vm;
pub mod weather;
pub use support::{Context, Error, Handle, Result};
