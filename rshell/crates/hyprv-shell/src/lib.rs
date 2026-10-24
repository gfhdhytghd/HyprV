//! Shell host integration and component attachment API.
pub mod host;
pub mod theme;

pub mod desktop_cli;
pub mod services;
pub mod startup_confirm;
pub mod system_bus;

pub mod assembly;
mod mapping;

pub mod nested;

mod assets;
pub mod runtime_assets;

pub mod fixture;

pub mod fullscreen;
mod compositor_capture;

pub mod compat;

mod laptop;

pub mod allocator;
