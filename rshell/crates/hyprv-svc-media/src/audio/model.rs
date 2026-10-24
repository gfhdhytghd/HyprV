//! Pure PipeWire audio model: default-sink tracking, cubic volume and write planning.
//! Mirrors plasma-pa: follow `default.audio.sink` metadata, read SPA Props, write through
//! the device Route when the node is backed by a card profile device (like pipewire-pulse).
use super::{AudioAction, AudioState};
use pipewire::spa::{pod::{Object, Property, Value, ValueArray}, sys};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SinkInfo {
    pub id: u32,
    pub name: String,
    pub description: String,
    /// `device.id`: global of the owning device.
    pub device_id: Option<u32>,
    /// `card.profile.device`: route device index on that device.
    pub profile_device: Option<i32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct NodeVolume {
    /// Linear (amplitude³) channel volumes as stored in SPA Props.
    channels: Option<Vec<f32>>,
    mute: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteInfo {
    pub index: i32,
    pub device: i32,
    pub output: bool,
}

/// Where and what to write for one action.
#[derive(Clone, Debug, PartialEq)]
pub struct VolumeWrite {
    pub node_id: u32,
    /// (device global, route index, route device) when writing through the card.
    pub route: Option<(u32, i32, i32)>,
    pub channels: Option<Vec<f32>>,
    pub mute: Option<bool>,
}

#[derive(Default)]
pub struct AudioModel {
    pub connected: bool,
    sinks: HashMap<u32, SinkInfo>,
    default_sink: Option<String>,
    volumes: HashMap<u32, NodeVolume>,
    routes: HashMap<u32, Vec<RouteInfo>>,
}

/// QML `controls.lua` step: `wpctl set-volume 5%+/-`.
pub const STEP: f64 = 0.05;
/// `wpctl -l 1` limit for set/increase; decrease has no limit (may stay above 100%).
pub const LIMIT: f64 = 1.0;

impl AudioModel {
    pub fn reset(&mut self) { *self = Self::default(); }
    pub fn add_sink(&mut self, sink: SinkInfo) { self.sinks.insert(sink.id, sink); }
    pub fn is_sink(&self, id: u32) -> bool { self.sinks.contains_key(&id) }
    pub fn remove_global(&mut self, id: u32) {
        self.sinks.remove(&id);
        self.volumes.remove(&id);
        self.routes.remove(&id);
    }
    /// Metadata `default.audio.sink` value is JSON `{"name":"..."}`; None clears it.
    pub fn set_default_sink_json(&mut self, value: Option<&str>) {
        self.default_sink = value.and_then(|v| {
            serde_json::from_str::<serde_json::Value>(v).ok()?.get("name")?.as_str().map(str::to_owned)
        });
    }
    pub fn default_sink(&self) -> Option<&SinkInfo> {
        let name = self.default_sink.as_deref()?;
        self.sinks.values().find(|s| s.name == name)
    }
    /// Merge a Props object; partial objects only update the keys they carry.
    pub fn update_props(&mut self, node: u32, props: &Value) {
        let Value::Object(object) = props else { return };
        let entry = self.volumes.entry(node).or_default();
        for p in &object.properties {
            match (p.key, &p.value) {
                (sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(v))) if !v.is_empty() => {
                    entry.channels = Some(v.clone())
                }
                (sys::SPA_PROP_mute, Value::Bool(m)) => entry.mute = Some(*m),
                _ => {}
            }
        }
    }
    /// Replace the known Route set of a device with one parsed Route object.
    pub fn update_route(&mut self, device: u32, route: &Value, first: bool) {
        let Value::Object(object) = route else { return };
        let mut info = RouteInfo { index: -1, device: -1, output: false };
        for p in &object.properties {
            match (p.key, &p.value) {
                (sys::SPA_PARAM_ROUTE_index, Value::Int(i)) => info.index = *i,
                (sys::SPA_PARAM_ROUTE_device, Value::Int(d)) => info.device = *d,
                (sys::SPA_PARAM_ROUTE_direction, Value::Id(id)) => info.output = id.0 == sys::SPA_DIRECTION_OUTPUT,
                _ => {}
            }
        }
        let routes = self.routes.entry(device).or_default();
        if first {
            routes.clear();
        }
        routes.retain(|r| r.device != info.device || r.output != info.output);
        if info.index >= 0 && info.device >= 0 {
            routes.push(info);
        }
    }

    pub fn state(&self) -> AudioState {
        let sink = self.default_sink();
        let volume = sink.and_then(|s| self.volumes.get(&s.id));
        let channels = volume.and_then(|v| v.channels.as_ref());
        let available = self.connected && sink.is_some() && channels.is_some();
        let cubic: Vec<f32> = channels.map(|c| c.iter().map(|v| v.max(0.0).cbrt()).collect()).unwrap_or_default();
        let level = if cubic.is_empty() { 0.0 } else { cubic.iter().map(|v| *v as f64).sum::<f64>() / cubic.len() as f64 };
        AudioState {
            ready: self.connected,
            available,
            muted: available && volume.and_then(|v| v.mute).unwrap_or(false),
            volume_percent: if available { (level * 100.0).round().max(0.0) as i32 } else { 0 },
            volume: if available { level } else { 0.0 },
            channel_volumes: if available { cubic } else { Vec::new() },
            sink_id: sink.map(|s| s.id),
            sink_name: sink.map(|s| s.name.clone()),
            sink_description: sink.map(|s| s.description.clone()),
        }
    }

    pub fn plan(&self, action: &AudioAction) -> Result<VolumeWrite, String> {
        let sink = self.default_sink().ok_or("no default audio sink")?;
        let volume = self.volumes.get(&sink.id).ok_or("default sink has no volume yet")?;
        let channels = volume.channels.as_ref().ok_or("default sink has no channel volumes")?;
        let cubic: Vec<f64> = channels.iter().map(|v| (v.max(0.0) as f64).cbrt()).collect();
        let current = cubic.iter().sum::<f64>() / cubic.len() as f64;
        let (target, mute) = match *action {
            // controls.lua: clamp(value,0,100)/100 formatted %.2f, limit 1.0
            AudioAction::SetVolume(percent) => {
                if !percent.is_finite() {
                    return Err("volume is not finite".into());
                }
                (Some((percent.clamp(0.0, 100.0) / 100.0 * 100.0).round() / 100.0), None)
            }
            AudioAction::Increase => (Some((current + STEP).min(LIMIT.max(0.0)).max(0.0)), None),
            AudioAction::Decrease => (Some((current - STEP).max(0.0)), None),
            AudioAction::ToggleMute => (None, Some(!volume.mute.unwrap_or(false))),
            AudioAction::SetMute(m) => (None, Some(m)),
        };
        // `wpctl -l 1`: an increase from above the limit clamps down to it.
        let channels = target.map(|t| scale_balanced(&cubic, current, t));
        let route = match (sink.device_id, sink.profile_device) {
            (Some(device), Some(profile_device)) => self
                .routes
                .get(&device)
                .and_then(|routes| routes.iter().find(|r| r.output && r.device == profile_device))
                .map(|r| (device, r.index, r.device)),
            _ => None,
        };
        Ok(VolumeWrite { node_id: sink.id, route, channels, mute })
    }
}

/// Keep channel balance (plasma-pa `pa_cvolume_scale` semantics, on the average):
/// the cubic average becomes `target`; a silent sink sets every channel to `target`.
fn scale_balanced(cubic: &[f64], current: f64, target: f64) -> Vec<f32> {
    cubic
        .iter()
        .map(|c| {
            let v = if current > 1e-6 { c * target / current } else { target };
            (v.max(0.0).powi(3)) as f32
        })
        .collect()
}

pub fn props_object(write: &VolumeWrite) -> Value {
    let mut properties = Vec::new();
    if let Some(channels) = &write.channels {
        properties.push(Property::new(sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(channels.clone()))));
    }
    if let Some(mute) = write.mute {
        properties.push(Property::new(sys::SPA_PROP_mute, Value::Bool(mute)));
    }
    Value::Object(Object { type_: sys::SPA_TYPE_OBJECT_Props, id: sys::SPA_PARAM_Props, properties })
}

pub fn route_object(index: i32, device: i32, props: Value) -> Value {
    Value::Object(Object {
        type_: sys::SPA_TYPE_OBJECT_ParamRoute,
        id: sys::SPA_PARAM_Route,
        properties: vec![
            Property::new(sys::SPA_PARAM_ROUTE_index, Value::Int(index)),
            Property::new(sys::SPA_PARAM_ROUTE_device, Value::Int(device)),
            Property::new(sys::SPA_PARAM_ROUTE_props, props),
            Property::new(sys::SPA_PARAM_ROUTE_save, Value::Bool(true)),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipewire::spa::utils::Id;

    fn props(channels: &[f32], mute: bool) -> Value {
        Value::Object(Object {
            type_: sys::SPA_TYPE_OBJECT_Props,
            id: sys::SPA_PARAM_Props,
            properties: vec![
                Property::new(sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(channels.to_vec()))),
                Property::new(sys::SPA_PROP_mute, Value::Bool(mute)),
            ],
        })
    }
    fn model() -> AudioModel {
        let mut m = AudioModel { connected: true, ..Default::default() };
        m.add_sink(SinkInfo { id: 50, name: "alsa_output.hw".into(), description: "Speakers".into(), device_id: Some(40), profile_device: Some(3) });
        m.add_sink(SinkInfo { id: 51, name: "bluez".into(), description: "Headset".into(), device_id: None, profile_device: None });
        m.set_default_sink_json(Some(r#"{"name":"alsa_output.hw"}"#));
        m.update_props(50, &props(&[0.125, 0.125], false)); // cubic 0.5
        m
    }

    #[test]
    fn state_is_cubic_and_rounded() {
        let s = model().state();
        assert!(s.available && s.ready && !s.muted);
        assert_eq!(s.volume_percent, 50);
        assert_eq!(s.sink_name.as_deref(), Some("alsa_output.hw"));
    }
    #[test]
    fn unavailable_without_default_or_props() {
        let mut m = model();
        m.set_default_sink_json(Some(r#"{"name":"missing"}"#));
        let s = m.state();
        assert!(!s.available && s.volume_percent == 0 && !s.muted);
        m.set_default_sink_json(Some(r#"{"name":"bluez"}"#));
        assert!(!m.state().available); // no Props yet
        m.set_default_sink_json(Some("garbage"));
        assert_eq!(m.state().sink_id, None);
    }
    #[test]
    fn over_100_percent_is_reported() {
        let mut m = model();
        m.update_props(50, &props(&[1.728, 1.728], true)); // 1.2³
        let s = m.state();
        assert_eq!(s.volume_percent, 120);
        assert!(s.muted);
    }
    #[test]
    fn partial_props_merge() {
        let mut m = model();
        m.update_props(50, &Value::Object(Object { type_: sys::SPA_TYPE_OBJECT_Props, id: sys::SPA_PARAM_Props, properties: vec![Property::new(sys::SPA_PROP_mute, Value::Bool(true))] }));
        let s = m.state();
        assert!(s.muted && s.volume_percent == 50);
    }
    #[test]
    fn set_increase_decrease_limits() {
        let mut m = model();
        let w = m.plan(&AudioAction::SetVolume(150.0)).unwrap();
        assert_eq!(w.channels.unwrap(), vec![1.0, 1.0]);
        let w = m.plan(&AudioAction::SetVolume(33.333)).unwrap();
        assert!((w.channels.unwrap()[0] as f64 - 0.33f64.powi(3)).abs() < 1e-6);
        m.update_props(50, &props(&[0.9f32.powi(3); 2], false));
        let w = m.plan(&AudioAction::Increase).unwrap();
        assert!((w.channels.unwrap()[0] as f64 - 0.95f64.powi(3)).abs() < 1e-5);
        m.update_props(50, &props(&[0.98f32.powi(3); 2], false));
        assert!((m.plan(&AudioAction::Increase).unwrap().channels.unwrap()[0] - 1.0).abs() < 1e-6);
        m.update_props(50, &props(&[1.2f32.powi(3); 2], false));
        assert!((m.plan(&AudioAction::Increase).unwrap().channels.unwrap()[0] - 1.0).abs() < 1e-6);
        let w = m.plan(&AudioAction::Decrease).unwrap();
        assert!((w.channels.unwrap()[0] as f64 - 1.15f64.powi(3)).abs() < 1e-4);
        m.update_props(50, &props(&[0.0, 0.0], false));
        assert_eq!(m.plan(&AudioAction::Decrease).unwrap().channels.unwrap(), vec![0.0, 0.0]);
        assert!(m.plan(&AudioAction::SetVolume(f64::NAN)).is_err());
    }
    #[test]
    fn balance_is_preserved() {
        let mut m = model();
        m.update_props(50, &props(&[0.4f32.powi(3), 0.6f32.powi(3)], false)); // avg 0.5
        let w = m.plan(&AudioAction::SetVolume(100.0)).unwrap();
        let c: Vec<f64> = w.channels.unwrap().iter().map(|v| (*v as f64).cbrt()).collect();
        assert!((c[0] - 0.8).abs() < 1e-5 && (c[1] - 1.2).abs() < 1e-5);
    }
    #[test]
    fn mute_toggle_and_route_target() {
        let mut m = model();
        let w = m.plan(&AudioAction::ToggleMute).unwrap();
        assert_eq!((w.mute, w.channels.clone(), w.route), (Some(true), None, None));
        let route = Value::Object(Object {
            type_: sys::SPA_TYPE_OBJECT_ParamRoute,
            id: sys::SPA_PARAM_Route,
            properties: vec![
                Property::new(sys::SPA_PARAM_ROUTE_index, Value::Int(7)),
                Property::new(sys::SPA_PARAM_ROUTE_direction, Value::Id(Id(sys::SPA_DIRECTION_OUTPUT))),
                Property::new(sys::SPA_PARAM_ROUTE_device, Value::Int(3)),
            ],
        });
        m.update_route(40, &route, true);
        assert_eq!(m.plan(&AudioAction::SetMute(false)).unwrap().route, Some((40, 7, 3)));
        m.remove_global(40);
        assert_eq!(m.plan(&AudioAction::SetMute(false)).unwrap().route, None);
        m.remove_global(50);
        assert!(m.plan(&AudioAction::ToggleMute).is_err());
    }
    #[test]
    fn write_objects_serialize() {
        use pipewire::spa::pod::serialize::PodSerializer;
        let w = model().plan(&AudioAction::SetVolume(20.0)).unwrap();
        let value = route_object(1, 2, props_object(&w));
        let bytes = PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &value).unwrap().0.into_inner();
        let (_, back) = pipewire::spa::pod::deserialize::PodDeserializer::deserialize_any_from(&bytes).unwrap();
        assert_eq!(back, value);
    }
}
