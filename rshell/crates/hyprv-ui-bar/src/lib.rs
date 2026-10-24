//! Bar, corner overlays and bar-owned popups. No provider, filesystem or socket I/O.
pub mod format;
pub mod input;
pub mod layout;
pub mod model;
pub mod motion;
pub mod popups;
pub mod surfaces;
pub mod tooltip;
pub mod view;
pub use model::{Action, Model, Theme};
pub use view::{build, update};
pub mod controller;
pub use controller::Controller;
pub mod ipc;
pub use input::{Input, InputResult, PopupCommand};
pub use motion::Motion;
pub use popups::{Kind as PopupKind, PopupInput, Popups};
pub use surfaces::Requirement;

pub mod baked;

pub mod svg;
