//! Native power and telemetry actors. Mutations occur only through explicit Actions.
pub mod actor;
pub mod backlight;
pub mod battery_history;
pub mod fan;
pub mod inhibit;
pub mod power;
pub mod sysstats;
pub use actor::{Context, Handle, Paths, Result};

/// Shared, process-latched policy owned by hyprv-core. Kept as a re-export for
/// callers of the explicit-profile test constructors.
pub use hyprv_core::profile::Profile;

#[cfg(test)]
mod test_support;
