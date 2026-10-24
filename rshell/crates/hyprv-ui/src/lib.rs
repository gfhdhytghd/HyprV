#![doc = include_str!("../README.md")]
//! HyprV's retained-mode, frame-driven Skia and Wayland UI engine.
pub mod animation;
pub mod egl;
pub mod geometry;
pub mod reactive;
pub mod render;
pub mod scene;
pub mod wayland;

/// Skia types used by scene content (paths, images, SVG, matrices, text alignment).
pub use skia_safe as skia;

mod qt_bezier;

pub mod effects;

pub mod text;

mod distance_field;

pub mod canvas;
pub mod image;
mod qt_canvas;
/// Qt Quick rectangle geometry and its explicit rendering mode.
pub mod qt_rect;

pub mod scroll;

pub mod qt_painter;

mod native_gl;

mod qt_gl_rect;
mod qt_gl_glyphs;

mod qt_gl_effect;

#[cfg(test)]
mod renderer_regressions;

/// Shared offscreen CPU/production-EGL backend selection.
pub mod offscreen;

/// Opt-in timing diagnostics; disabled unless HYPRV_UI_PROFILE is set.
pub mod diagnostics;

/// Qt-compatible PNG serialization for reference capture transports.
pub mod capture;



