//! Event-driven shell infrastructure. No module starts polling implicitly.
pub mod clock;
pub mod config;
pub mod ipc;
pub mod runtime;
pub mod service;
pub mod store;
pub use calloop;
pub use tokio;

pub mod sample_file;

/// Latched desktop/laptop policy and isolated native filesystem root.
pub mod profile;
