//! Compatibility import for external QA tools. The production offscreen target
//! and HYPRV_PARITY_BACKEND selection now live in hyprv-ui (ENG-PQ8).
//! New harnesses use Renderer::{raster,image,png} or hyprv_ui::offscreen directly.
pub use hyprv_ui::offscreen::*;
