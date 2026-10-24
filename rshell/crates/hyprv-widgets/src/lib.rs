//! Shared, specification-driven retained widgets for HyprV.
pub mod containers;
#[cfg(feature = "controls")]
pub mod controls;
pub mod foundations;
pub mod icon_theme;
pub mod qt_svg;
pub mod primitives;
#[cfg(feature = "viz")]
pub mod viz;
pub use hyprv_ui;

mod color_math;
pub mod flickable;
