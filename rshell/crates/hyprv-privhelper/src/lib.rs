//! Shared versioned fan protocol and cooling policy. No untrusted paths on D-Bus.
pub mod cooling;
pub const BUS: &str = "org.hyprv.Shell.PrivHelper";
pub const PATH: &str = "/org/hyprv/Shell/PrivHelper";
pub const INTERFACE: &str = "org.hyprv.Shell.PrivHelper1";
pub const VERSION: u32 = 1;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FanChannel {
    Chassis,
    Pump,
}
impl FanChannel {
    pub fn index(self) -> u8 {
        match self {
            Self::Chassis => 1,
            Self::Pump => 7,
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "chassis" => Some(Self::Chassis),
            "pump" => Some(Self::Pump),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FanReading {
    pub channel: FanChannel,
    pub limit_percent: u8,
    pub pwm: u8,
    pub rpm: Option<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FanState {
    pub available: bool,
    pub chip: String,
    pub temperature_c: Option<f64>,
    pub desktop_policy: bool,
    pub channels: Vec<FanReading>,
    pub last_error: Option<String>,
}
pub mod egpu;
pub mod events;

pub mod rapl;
pub mod cpu_power;
pub mod eligibility;
