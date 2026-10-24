mod laptop;
use crate::actor::{self, Backend, Context, Handle, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BatteryMode {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Full,
    Idle,
    Plugged,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
    Other(String),
}
impl Default for PowerProfile {
    fn default() -> Self {
        Self::Other("unknown".into())
    }
}
impl PowerProfile {
    pub fn parse(s: &str) -> Self {
        match s {
            "power-saver" => Self::PowerSaver,
            "balanced" => Self::Balanced,
            "performance" => Self::Performance,
            s => Self::Other(s.into()),
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
            Self::Other(s) => s,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub available: bool,
    pub on_battery: bool,
    pub battery_present: bool,
    pub battery_percent: Option<f64>,
    pub battery_mode: BatteryMode,
    pub profile: PowerProfile,
    pub profiles: Vec<PowerProfile>,
    pub performance_degraded: Option<String>,
    pub sleeping: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lid_present: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lid_closed: Option<bool>,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub energy_rate_w: Option<f64>,
    pub time_to_empty_s: Option<i64>,
    pub time_to_full_s: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Refresh,
    ProfileSet(PowerProfile),
    ProfileToggle,
    /// Legacy control-panel order, distinct from the CLI toggle order.
    ProfileCyclePanel,
    Suspend,
}
pub fn battery_mode(present: bool, state: u32, on_battery: bool) -> BatteryMode {
    if !present {
        BatteryMode::Unknown
    } else {
        match state {
            1 | 5 => BatteryMode::Charging,
            2 | 6 => BatteryMode::Discharging,
            4 => BatteryMode::Full,
            _ if on_battery => BatteryMode::Idle,
            _ => BatteryMode::Plugged,
        }
    }
}
pub fn next_profile(profile: &PowerProfile) -> PowerProfile {
    match profile {
        PowerProfile::Performance => PowerProfile::Balanced,
        PowerProfile::Balanced => PowerProfile::PowerSaver,
        PowerProfile::PowerSaver => PowerProfile::Performance,
        PowerProfile::Other(_) => PowerProfile::Balanced,
    }
}
pub fn next_panel_profile(profile: &PowerProfile) -> PowerProfile {
    match profile {
        PowerProfile::PowerSaver => PowerProfile::Balanced,
        PowerProfile::Balanced => PowerProfile::Performance,
        PowerProfile::Performance => PowerProfile::PowerSaver,
        PowerProfile::Other(_) => PowerProfile::Balanced,
    }
}
pub fn spawn_for_profile(ctx: Context, profile: crate::Profile) -> Handle<State, Action> {
    match profile {
        crate::Profile::Desktop => spawn_desktop(ctx),
        crate::Profile::Laptop => actor::spawn(ctx.clone(), laptop::Native::new(ctx), false),
    }
}
pub fn spawn(ctx: Context) -> Handle<State, Action> {
    spawn_for_profile(ctx, hyprv_core::profile::profile())
}
fn spawn_desktop(ctx: Context) -> Handle<State, Action> {
    actor::spawn(
        ctx.clone(),
        Native {
            ctx,
            upower: None,
            profiles: None,
            modern: None,
            login: None,
            profile_provider: None,
        },
        false,
    )
}
struct Native {
    ctx: Context,
    upower: Option<actor::SignalInbox>,
    profiles: Option<actor::SignalInbox>,
    modern: Option<actor::SignalInbox>,
    login: Option<actor::SignalInbox>,
    profile_provider: Option<(String, String)>,
}
impl Backend for Native {
    fn providers(&self) -> &'static [&'static str] {
        &[
            "org.freedesktop.UPower",
            "org.freedesktop.login1",
            "net.hadess.PowerProfiles",
            "org.freedesktop.UPower.PowerProfiles",
        ]
    }
    type State = State;
    type Action = Action;
    async fn connect(&mut self) -> Result<()> {
        self.upower = Some(actor::signals(&self.ctx.bus, "org.freedesktop.UPower").await?);
        self.login = Some(actor::signals(&self.ctx.bus, "org.freedesktop.login1").await?);
        self.profiles = Some(actor::signals(&self.ctx.bus, "net.hadess.PowerProfiles").await?);
        self.modern =
            Some(actor::signals(&self.ctx.bus, "org.freedesktop.UPower.PowerProfiles").await?);
        Ok(())
    }
    async fn snapshot(&mut self) -> Result<State> {
        // PrepareForSleep is not replayed after a bounded inbox overflow.
        let login = actor::proxy(
            &self.ctx.bus,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        if let Ok(value) = login.get_property::<bool>("PreparingForSleep").await {
            self.ctx.sleeping.send_if_modified(|current| {
                if *current == value {
                    false
                } else {
                    *current = value;
                    true
                }
            });
        }
        let mut state = State {
            sleeping: *self.ctx.sleeping.borrow(),
            ..Default::default()
        };
        let root = actor::proxy(
            &self.ctx.bus,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .await?;
        if let Ok(on_battery) = root.get_property::<bool>("OnBattery").await {
            state.available = true;
            state.on_battery = on_battery;
            if let Ok(path) = root
                .call::<_, _, zbus::zvariant::OwnedObjectPath>("GetDisplayDevice", &())
                .await
            {
                let device = zbus::Proxy::new(
                    &self.ctx.bus,
                    "org.freedesktop.UPower",
                    path,
                    "org.freedesktop.UPower.Device",
                )
                .await?;
                let present = device
                    .get_property::<bool>("IsPresent")
                    .await
                    .unwrap_or(false);
                let kind = device.get_property::<u32>("Type").await.unwrap_or(0);
                state.battery_present = present && kind == 2;
                state.battery_mode = battery_mode(
                    state.battery_present,
                    device.get_property("State").await.unwrap_or(0),
                    on_battery,
                );
                if state.battery_present {
                    state.battery_percent = device
                        .get_property::<f64>("Percentage")
                        .await
                        .ok()
                        .filter(|x| x.is_finite())
                        .map(|x| x.clamp(0., 100.));
                    state.energy_now_wh = device.get_property("Energy").await.ok();
                    state.energy_full_wh = device.get_property("EnergyFull").await.ok();
                    state.energy_rate_w = device.get_property("EnergyRate").await.ok();
                    state.time_to_empty_s = device.get_property("TimeToEmpty").await.ok();
                    state.time_to_full_s = device.get_property("TimeToFull").await.ok();
                }
            }
        }
        self.profile_provider = None;
        for (bus, path) in [
            ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
            (
                "org.freedesktop.UPower.PowerProfiles",
                "/org/freedesktop/UPower/PowerProfiles",
            ),
        ] {
            let interface = bus;
            let proxy = actor::proxy(&self.ctx.bus, bus, path, interface).await?;
            if let Ok(current) = proxy.get_property::<String>("ActiveProfile").await {
                state.profile = PowerProfile::parse(&current);
                let profiles: Vec<std::collections::HashMap<String, zbus::zvariant::OwnedValue>> =
                    proxy.get_property("Profiles").await?;
                state.profiles = profiles
                    .iter()
                    .filter_map(|p| {
                        p.get("Profile")
                            .and_then(|v| <&str>::try_from(v).ok())
                            .map(PowerProfile::parse)
                    })
                    .collect();
                state.performance_degraded = proxy
                    .get_property::<String>("PerformanceDegraded")
                    .await
                    .ok()
                    .filter(|s| !s.is_empty());
                self.profile_provider = Some((bus.into(), path.into()));

                break;
            }
        }
        Ok(state)
    }
    async fn apply(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Refresh => Ok(()),
            Action::Suspend => {
                actor::proxy(
                    &self.ctx.bus,
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                )
                .await?
                .call::<_, _, ()>("Suspend", &(true,))
                .await?;
                Ok(())
            }
            action => {
                let state = self.snapshot().await?;
                let profile = match action {
                    Action::ProfileSet(p) => p,
                    Action::ProfileToggle => next_profile(&state.profile),
                    Action::ProfileCyclePanel => next_panel_profile(&state.profile),
                    _ => unreachable!(),
                };
                if !state.profiles.contains(&profile) {
                    return Err("profile not offered by provider".into());
                }
                let (bus, path) = self
                    .profile_provider
                    .as_ref()
                    .ok_or("power profile provider absent")?;
                let proxy = actor::proxy(&self.ctx.bus, bus, path, bus).await?;
                proxy.set_property("ActiveProfile", profile.name()).await?;
                Ok(())
            }
        }
    }
    async fn event(&mut self) -> Result<()> {
        tokio::select! {r=actor::next(&mut self.upower)=>r,r=actor::next(&mut self.profiles)=>r,r=actor::next(&mut self.modern)=>r,m=async{self.login.as_mut().unwrap().next().await}=>{let message=m.ok_or("logind disconnected")??;if message.header().member().is_some_and(|s|s.as_str()=="PrepareForSleep"){let(sleeping,):(bool,)=message.body().deserialize()?;self.ctx.sleeping.send_replace(sleeping);}Ok(())}}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mappings() {
        assert_eq!(battery_mode(true, 5, false), BatteryMode::Charging);
        assert_eq!(battery_mode(true, 6, true), BatteryMode::Discharging);
        assert_eq!(battery_mode(false, 4, false), BatteryMode::Unknown);
        assert_eq!(
            next_profile(&PowerProfile::Performance),
            PowerProfile::Balanced
        );
        assert_eq!(PowerProfile::parse("custom").name(), "custom");
    }
}
