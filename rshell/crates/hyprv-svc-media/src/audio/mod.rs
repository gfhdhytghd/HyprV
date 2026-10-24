//! Default-sink volume/mute over native PipeWire (replaces `Pipewire.defaultAudioSink`
//! in `shell.qml:3285`/`BarServices.qml:49` and `wpctl` in `hypr/controls.lua:152`).
//!
//! One PipeWire connection on a dedicated thread: registry globals, the `default`
//! metadata object and per-sink Props/device Route subscriptions. The thread blocks in
//! the PipeWire loop; there are no timers except reconnect backoff while PipeWire is absent.
pub mod model;

use crate::common::{action_queue, err, Backoff, Context, Handle, Reply, Result};
use hyprv_core::store::TopicKey;
use model::{AudioModel, SinkInfo};
use pipewire as pw;
use pw::spa::{param::ParamType, pod::{deserialize::PodDeserializer, serialize::PodSerializer, Pod, Value}};
use serde::{Deserialize, Serialize};
use std::{any::Any, cell::RefCell, collections::HashMap, rc::Rc};

/// QML: `audioAvailable`, `audioMuted`, `audioVolumePercent` (`shell.qml:3292`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioState {
    /// `Pipewire.ready`: connected and registry enumerated.
    pub ready: bool,
    /// `Pipewire.ready && defaultAudioSink.ready && defaultAudioSink.audio`.
    pub available: bool,
    /// `available && audio.muted`.
    pub muted: bool,
    /// `available ? max(0, round(volume*100)) : 0`; may exceed 100.
    pub volume_percent: i32,
    /// Quickshell cubic volume: mean of per-channel cbrt(linear).
    pub volume: f64,
    /// Per-channel cubic volumes.
    pub channel_volumes: Vec<f32>,
    pub sink_id: Option<u32>,
    pub sink_name: Option<String>,
    pub sink_description: Option<String>,
}

/// `require("hypr.controls").volume(...)` arms used by the shell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AudioAction {
    /// `volume("set", N)`: clamp 0..100, two decimals, limit 1.0.
    SetVolume(f64),
    /// `volume("inc")`: +5%, limit 1.0.
    Increase,
    /// `volume("dec")`: -5%, no upper limit.
    Decrease,
    /// `volume("toggle")`.
    ToggleMute,
    SetMute(bool),
}

pub struct AudioTopic;
impl TopicKey for AudioTopic { type Value = AudioState; }
pub type AudioHandle = Handle<AudioState, AudioAction>;

enum Msg {
    Action(AudioAction, Reply<()>),
    Quit,
}

/// Audio is part of the always-visible bar; the connection is event driven and idle-free,
/// so it is held for the whole runtime rather than per subscriber.
pub fn spawn(ctx: &Context) -> AudioHandle {
    let topic = ctx.store.topic::<AudioTopic>(AudioState::default());
    let (actions, mut queue) = action_queue::<AudioAction, ()>();
    let mut shutdown = ctx.runtime.shutdown_signal();
    let read_only = ctx.read_only;
    let publish = topic.clone();
    let task = ctx.runtime.spawn(async move {
        let (sender, receiver) = pw::channel::channel::<Msg>();
        let thread = std::thread::Builder::new()
            .name("hyprv-pw-audio".into())
            .spawn(move || pw_thread(publish, receiver, read_only));
        let Ok(thread) = thread else { return };
        loop {
            tokio::select! {
                request = queue.recv() => match request {
                    Some((action, reply)) => {
                        if let Err(Msg::Action(_, reply)) = sender.send(Msg::Action(action, reply)) {
                            let _ = reply.send(err("audio thread stopped"));
                        }
                    }
                    None => break,
                },
                changed = shutdown.changed() => if changed.is_err() || *shutdown.borrow() { break },
            }
        }
        let _ = sender.send(Msg::Quit);
        let _ = tokio::task::spawn_blocking(move || thread.join()).await;
    });
    Handle::new(topic, actions, task)
}

struct Shared {
    model: AudioModel,
    topic: hyprv_core::store::Topic<AudioState>,
    nodes: HashMap<u32, pw::node::Node>,
    devices: HashMap<u32, pw::device::Device>,
    keep: HashMap<u32, Vec<Box<dyn Any>>>,
}
impl Shared {
    fn publish(&self) { self.topic.publish(self.model.state()); }
}

enum Exit { Quit, Disconnected }

fn pw_thread(topic: hyprv_core::store::Topic<AudioState>, mut receiver: pw::channel::Receiver<Msg>, read_only: bool) {
    pw::init();
    let mut backoff = Backoff::new();
    loop {
        let (exit, rx) = session(&topic, receiver, read_only, &mut backoff);
        receiver = rx;
        let mut state = topic.snapshot().value;
        state.ready = false;
        state.available = false;
        state.muted = false;
        state.volume_percent = 0;
        state.volume = 0.0;
        state.channel_volumes.clear();
        topic.publish(state);
        if matches!(exit, Exit::Quit) {
            return;
        }
        let (exit, rx) = wait(receiver, backoff.next_delay());
        receiver = rx;
        if matches!(exit, Exit::Quit) {
            return;
        }
    }
}

/// Interruptible backoff that still answers actions (with an error) and Quit.
fn wait(receiver: pw::channel::Receiver<Msg>, delay: std::time::Duration) -> (Exit, pw::channel::Receiver<Msg>) {
    let Ok(main_loop) = pw::main_loop::MainLoopRc::new(None) else {
        std::thread::sleep(delay);
        return (Exit::Disconnected, receiver);
    };
    let exit = Rc::new(RefCell::new(Exit::Disconnected));
    let weak = main_loop.downgrade();
    let timer = main_loop.loop_().add_timer(move |_| {
        if let Some(l) = weak.upgrade() { l.quit() }
    });
    let _ = timer.update_timer(Some(delay), None);
    let weak = main_loop.downgrade();
    let flag = exit.clone();
    let attached = receiver.attach(main_loop.loop_(), move |msg| match msg {
        Msg::Action(_, reply) => { let _ = reply.send(err("PipeWire unavailable")); }
        Msg::Quit => {
            *flag.borrow_mut() = Exit::Quit;
            if let Some(l) = weak.upgrade() { l.quit() }
        }
    });
    main_loop.run();
    let receiver = attached.deattach();
    drop(timer);
    let exit = std::mem::replace(&mut *exit.borrow_mut(), Exit::Disconnected);
    (exit, receiver)
}

fn session(
    topic: &hyprv_core::store::Topic<AudioState>,
    receiver: pw::channel::Receiver<Msg>,
    read_only: bool,
    backoff: &mut Backoff,
) -> (Exit, pw::channel::Receiver<Msg>) {
    let connected = (|| -> std::result::Result<_, pw::Error> {
        let main_loop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&main_loop, None)?;
        let core = context.connect_rc(None)?;
        let registry = core.get_registry_rc()?;
        Ok((main_loop, context, core, registry))
    })();
    let Ok((main_loop, _context, core, registry)) = connected else {
        return (Exit::Disconnected, receiver);
    };
    let shared = Rc::new(RefCell::new(Shared {
        model: AudioModel::default(),
        topic: topic.clone(),
        nodes: HashMap::new(),
        devices: HashMap::new(),
        keep: HashMap::new(),
    }));
    let exit = Rc::new(RefCell::new(Exit::Disconnected));

    // Disconnect (EPIPE on the core) ends this session; the outer loop reconnects.
    let weak = main_loop.downgrade();
    let _core_listener = core
        .add_listener_local()
        .error(move |id, _seq, _res, _message| {
            if id == pw::core::PW_ID_CORE {
                if let Some(l) = weak.upgrade() { l.quit() }
            }
        })
        .register();

    // `ready` after the first registry roundtrip, like Quickshell's Pipewire.ready.
    let pending = Rc::new(RefCell::new(None));
    if let Ok(seq) = core.sync(0) {
        *pending.borrow_mut() = Some(seq);
    }
    let sync_shared = shared.clone();
    let sync_pending = pending.clone();
    let _done_listener = core
        .add_listener_local()
        .done(move |id, seq| {
            if id == pw::core::PW_ID_CORE && Some(seq) == *sync_pending.borrow() {
                let mut s = sync_shared.borrow_mut();
                s.model.connected = true;
                s.publish();
            }
        })
        .register();

    let registry_weak = registry.downgrade();
    let global_shared = shared.clone();
    let remove_shared = shared.clone();
    let _registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            let Some(registry) = registry_weak.upgrade() else { return };
            let props = global.props;
            let get = |key: &str| props.and_then(|p| p.get(key)).map(str::to_owned);
            match global.type_ {
                pw::types::ObjectType::Node if get("media.class").as_deref() == Some("Audio/Sink") => {
                    let Ok(node) = registry.bind::<pw::node::Node, _>(global) else { return };
                    let id = global.id;
                    let sink = SinkInfo {
                        id,
                        name: get("node.name").unwrap_or_default(),
                        description: get("node.description").or_else(|| get("node.nick")).unwrap_or_default(),
                        device_id: get("device.id").and_then(|v| v.parse().ok()),
                        profile_device: get("card.profile.device").and_then(|v| v.parse().ok()),
                    };
                    let param_shared = global_shared.clone();
                    let listener = node
                        .add_listener_local()
                        .param(move |_seq, kind, _index, _next, pod| {
                            if kind != ParamType::Props { return }
                            let Some(value) = pod.and_then(decode) else { return };
                            let mut s = param_shared.borrow_mut();
                            s.model.update_props(id, &value);
                            s.publish();
                        })
                        .register();
                    node.subscribe_params(&[ParamType::Props]);
                    let mut s = global_shared.borrow_mut();
                    s.model.add_sink(sink);
                    s.nodes.insert(id, node);
                    s.keep.entry(id).or_default().push(Box::new(listener));
                    s.publish();
                }
                pw::types::ObjectType::Device if get("media.class").as_deref() == Some("Audio/Device") => {
                    let Ok(device) = registry.bind::<pw::device::Device, _>(global) else { return };
                    let id = global.id;
                    let param_shared = global_shared.clone();
                    let listener = device
                        .add_listener_local()
                        .param(move |_seq, kind, index, _next, pod| {
                            if kind != ParamType::Route { return }
                            let Some(value) = pod.and_then(decode) else { return };
                            param_shared.borrow_mut().model.update_route(id, &value, index == 0);
                        })
                        .register();
                    device.subscribe_params(&[ParamType::Route]);
                    let mut s = global_shared.borrow_mut();
                    s.devices.insert(id, device);
                    s.keep.entry(id).or_default().push(Box::new(listener));
                }
                pw::types::ObjectType::Metadata if get("metadata.name").as_deref() == Some("default") => {
                    let Ok(metadata) = registry.bind::<pw::metadata::Metadata, _>(global) else { return };
                    let meta_shared = global_shared.clone();
                    let listener = metadata
                        .add_listener_local()
                        .property(move |subject, key, _type, value| {
                            if subject != 0 { return 0 }
                            match key {
                                Some("default.audio.sink") | None => {
                                    let mut s = meta_shared.borrow_mut();
                                    s.model.set_default_sink_json(value);
                                    s.publish();
                                }
                                _ => {}
                            }
                            0
                        })
                        .register();
                    let mut s = global_shared.borrow_mut();
                    s.keep.entry(global.id).or_default().push(Box::new(listener));
                    s.keep.entry(global.id).or_default().push(Box::new(metadata));
                }
                _ => {}
            }
        })
        .global_remove(move |id| {
            let mut s = remove_shared.borrow_mut();
            let was_sink = s.model.is_sink(id);
            s.model.remove_global(id);
            s.nodes.remove(&id);
            s.devices.remove(&id);
            let dropped = s.keep.remove(&id);
            if was_sink { s.publish(); }
            drop(s);
            drop(dropped);
        })
        .register();

    let weak = main_loop.downgrade();
    let flag = exit.clone();
    let action_shared = shared.clone();
    let attached = receiver.attach(main_loop.loop_(), move |msg| match msg {
        Msg::Quit => {
            *flag.borrow_mut() = Exit::Quit;
            if let Some(l) = weak.upgrade() { l.quit() }
        }
        Msg::Action(action, reply) => {
            let result = if read_only { err("read-only context") } else { apply(&action_shared.borrow(), &action) };
            let _ = reply.send(result);
        }
    });
    backoff.reset();
    main_loop.run();
    let receiver = attached.deattach();
    {
        // Drop listeners before proxies, all before the core/loop.
        let mut s = shared.borrow_mut();
        s.keep.clear();
        s.nodes.clear();
        s.devices.clear();
    }
    let exit = std::mem::replace(&mut *exit.borrow_mut(), Exit::Disconnected);
    (exit, receiver)
}

fn decode(pod: &Pod) -> Option<Value> {
    PodDeserializer::deserialize_any_from(pod.as_bytes()).ok().map(|(_, v)| v)
}

fn encode(value: &Value) -> Option<Vec<u8>> {
    PodSerializer::serialize(std::io::Cursor::new(Vec::new()), value).ok().map(|(c, _)| c.into_inner())
}

/// Sends the SPA write. Success means "request sent"; observed Props confirm it.
fn apply(shared: &Shared, action: &AudioAction) -> Result<()> {
    let write = shared.model.plan(action).map_err(crate::Error)?;
    let props = model::props_object(&write);
    match write.route {
        Some((device_id, index, route_device)) => {
            let device = shared.devices.get(&device_id).ok_or_else(|| crate::Error("route device vanished".into()))?;
            let bytes = encode(&model::route_object(index, route_device, props)).ok_or_else(|| crate::Error("pod encode failed".into()))?;
            let pod = Pod::from_bytes(&bytes).ok_or_else(|| crate::Error("pod encode failed".into()))?;
            device.set_param(ParamType::Route, 0, pod);
        }
        None => {
            let node = shared.nodes.get(&write.node_id).ok_or_else(|| crate::Error("sink vanished".into()))?;
            let bytes = encode(&props).ok_or_else(|| crate::Error("pod encode failed".into()))?;
            let pod = Pod::from_bytes(&bytes).ok_or_else(|| crate::Error("pod encode failed".into()))?;
            node.set_param(ParamType::Props, 0, pod);
        }
    }
    Ok(())
}
