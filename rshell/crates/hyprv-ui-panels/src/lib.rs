//! HyprV panels and popups (docs/spec/ui-panels*.md): control panel (main, Bluetooth and
//! Mihomo pages), Wi-Fi popup, QuickAdjust OSD, system stats popup, week calendar and the
//! startup confirmation dialog. Pure UI: no provider, filesystem or socket I/O; actions
//! leave as service `Action` enums or host intents.
pub mod qtext;
pub mod qdf;
pub mod qrect;
pub mod baked;
pub mod model;
pub mod ui;
pub use model::{Action, HostAction, Model, PanelKind, Theme};
pub mod widgets;
pub mod popup;
pub mod control_panel;
pub mod fixtures;
pub mod icons;
pub mod wifi;
pub mod quick_adjust;
pub mod stats;
pub mod calendar;
pub mod startup;
pub mod panels;
pub use panels::Panels;
#[cfg(feature = "services")]
pub mod services;
pub mod assets;
