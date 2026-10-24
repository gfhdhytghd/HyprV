//! Demand-only spectrum (replaces `scripts/audio-spectrum.sh` + cava, `shell.qml:3239`).
//!
//! Capture runs only while the topic has subscribers (the media UI is presented) AND the
//! gate is open (selected player available and playing) — the QML `audioSpectrumProcess`
//! condition. While active it is exactly the legacy cava process: the vendored cavacore
//! plus cava's input/main-loop/output steps ([`cava`]), a PipeWire stream configured like
//! cava's PipeWire input, and a 24 Hz frame timer. Nothing runs while inactive; stopping
//! drops the stream and its buffer and clears `values`, like `onRunningChanged`.
pub mod cava;

use crate::common::{action_queue, bump, Context, Counters, Handle};
use hyprv_core::{service::{gated_demand, run_sampled, Service, ServiceFuture}, store::{Topic, TopicKey}};
use pipewire as pw;
use pw::{properties::properties, spa};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc, sync::Arc, thread::JoinHandle};
use tokio::sync::watch;


/// QML: `audioSpectrumValues` (16 bars, integers 0..100, `[]` when not running) and
/// `audioSpectrumProcess.running`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpectrumState {
    pub running: bool,
    pub values: Vec<f64>,
}

pub struct SpectrumTopic;
impl TopicKey for SpectrumTopic { type Value = SpectrumState; }
pub type SpectrumHandle = Handle<SpectrumState, std::convert::Infallible>;

/// `gate` must be true only while media is available and playing; see
/// [`crate::mpris::playing_gate`].
pub fn spawn(ctx: &Context, gate: watch::Receiver<bool>) -> SpectrumHandle {
    spawn_with(ctx, gate, capture)
}

/// Capture body run on the worker thread until its quit channel fires.
type Backend = fn(&Topic<SpectrumState>, pw::channel::Receiver<()>) -> Result<(), pw::Error>;

fn spawn_with(ctx: &Context, gate: watch::Receiver<bool>, backend: Backend) -> SpectrumHandle {
    let topic = ctx.store.topic::<SpectrumTopic>(SpectrumState::default());
    let (actions, _queue) = action_queue();
    let shutdown = ctx.runtime.shutdown_signal();
    // Effective demand = subscribers > 0 && gate (core `gated_demand`).
    let (wanted, driver) = gated_demand(topic.demand(), vec![gate], shutdown.clone());
    let service = Capture { topic: topic.clone(), worker: None, backend, counters: ctx.counters.clone() };
    // MEDIA-1: never drop `run_sampled` mid-capture. When the driver ends (shutdown or
    // gate closed) its demand sender closes, so `run_sampled` stops the capture itself.
    let task = ctx.runtime.spawn(async move {
        let _ = tokio::join!(driver, run_sampled(service, wanted, shutdown));
    });
    Handle::new(topic, actions, task)
}

/// A PipeWire loop thread owned RAII-style: dropping it (e.g. a cancelled lifecycle)
/// always tells the loop to quit, so no capture stream or frame timer outlives it.
struct Worker {
    quit: Option<pw::channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    fn spawn(name: &str, body: impl FnOnce(pw::channel::Receiver<()>) + Send + 'static) -> std::io::Result<Self> {
        let (quit, receiver) = pw::channel::channel::<()>();
        let thread = std::thread::Builder::new().name(name.into()).spawn(move || body(receiver))?;
        Ok(Self { quit: Some(quit), thread: Some(thread) })
    }
    /// Graceful stop: quit, then join off the service thread.
    async fn stop(mut self) {
        if let Some(quit) = self.quit.take() {
            let _ = quit.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = tokio::task::spawn_blocking(move || thread.join()).await;
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        // Cancellation path: quit without blocking; the thread exits on its own.
        if let Some(quit) = self.quit.take() {
            let _ = quit.send(());
        }
    }
}

struct Capture {
    topic: Topic<SpectrumState>,
    worker: Option<Worker>,
    backend: Backend,
    counters: Arc<Counters>,
}
impl Drop for Capture {
    fn drop(&mut self) {
        if self.worker.take().is_some() {
            self.topic.publish(SpectrumState::default());
        }
    }
}

impl Service for Capture {
    fn start(&mut self) -> ServiceFuture<'_> {
        Box::pin(async move {
            let topic = self.topic.clone();
            let backend = self.backend;
            bump(&self.counters.spectrum_starts);
            self.worker = Some(Worker::spawn("hyprv-pw-spectrum", move |receiver| {
                if backend(&topic, receiver).is_err() {
                    topic.publish(SpectrumState::default());
                }
            })?);
            Ok(())
        })
    }
    fn stop(&mut self) -> ServiceFuture<'_> {
        Box::pin(async move {
            if let Some(worker) = self.worker.take() {
                worker.stop().await;
            }
            self.topic.publish(SpectrumState::default());
            Ok(())
        })
    }
}

/// cava's PipeWire input (`input/pipewire.c`): S16 interleaved stereo at 48 kHz from the
/// default sink monitor, `node.latency` = next_pow2(BUFFER_SIZE·rate/48000)/rate,
/// always-process, virtual, remix to FL/FR with upmix. Frames run on a loop timer at
/// `framerate` like cava's main loop (frames without new samples still advance the
/// smoothing), only while capture is active.
fn capture(topic: &Topic<SpectrumState>, quit: pw::channel::Receiver<()>) -> Result<(), pw::Error> {
    pw::init();
    let config = cava::Config::default();
    let cava = Rc::new(RefCell::new(cava::Cava::new(config.clone()).map_err(|_| pw::Error::CreationFailed)?));
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_rc(None)?;
    let weak = main_loop.downgrade();
    let _quit = quit.attach(main_loop.loop_(), move |()| {
        if let Some(l) = weak.upgrade() { l.quit() }
    });
    let weak = main_loop.downgrade();
    let _core = core
        .add_listener_local()
        .error(move |id, _, _, _| {
            if id == pw::core::PW_ID_CORE {
                if let Some(l) = weak.upgrade() { l.quit() }
            }
        })
        .register();

    let latency = (512u32 * config.rate / 48_000).next_power_of_two();
    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::STREAM_CAPTURE_SINK => "true",
        *pw::keys::NODE_LATENCY => format!("{latency}/{}", config.rate),
        *pw::keys::NODE_ALWAYS_PROCESS => "true",
        *pw::keys::NODE_VIRTUAL => "true",
        *pw::keys::STREAM_DONT_REMIX => "false",
        "channelmix.upmix" => "true",
    };
    let stream = pw::stream::StreamRc::new(core.clone(), "hyprv-spectrum", props)?;
    let input = cava.clone();
    let _listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            let Some(first) = datas.first_mut() else { return };
            let size = first.chunk().size() as usize;
            let Some(bytes) = first.data() else { return };
            let samples: Vec<i16> = bytes[..size.min(bytes.len())].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            input.borrow_mut().push_s16(&samples);
        })
        .register()?;

    let frame_cava = cava.clone();
    let frame_topic = topic.clone();
    let timer = main_loop.loop_().add_timer(move |_| {
        let values = frame_cava.borrow_mut().frame().into_iter().map(f64::from).collect();
        frame_topic.publish(SpectrumState { running: true, values });
    });
    let period = std::time::Duration::from_nanos(1_000_000_000 / u64::from(config.framerate));
    let _ = timer.update_timer(Some(period), Some(period));

    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::S16LE);
    info.set_rate(config.rate);
    info.set_channels(config.channels as u32);
    let mut position = [0u32; spa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    info.set_position(position);
    let object = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    let bytes = spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(object))
        .map_err(|_| pw::Error::CreationFailed)?
        .0
        .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).ok_or(pw::Error::CreationFailed)?];
    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;
    topic.publish(SpectrumState { running: true, values: Vec::new() });
    main_loop.run();
    drop(timer);
    let _ = stream.disconnect();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};

    /// A loop with a 5 ms timer standing in for the capture loop (no PipeWire daemon needed).
    fn ticking_worker(count: Arc<AtomicUsize>) -> Worker {
        Worker::spawn("test-spectrum", move |quit| {
            pw::init();
            let main_loop = pw::main_loop::MainLoopRc::new(None).unwrap();
            let weak = main_loop.downgrade();
            let _quit = quit.attach(main_loop.loop_(), move |()| {
                if let Some(l) = weak.upgrade() { l.quit() }
            });
            let timer = main_loop.loop_().add_timer(move |_| { count.fetch_add(1, Ordering::SeqCst); });
            let period = std::time::Duration::from_millis(5);
            let _ = timer.update_timer(Some(period), Some(period));
            main_loop.run();
        })
        .unwrap()
    }

    #[test]
    fn dropping_capture_quits_its_loop_and_timer() {
        let count = Arc::new(AtomicUsize::new(0));
        let topic = Topic::new(SpectrumState { running: true, values: vec![1.; 16] });
        let capture = Capture { topic: topic.clone(), worker: Some(ticking_worker(count.clone())), backend: super::capture, counters: Arc::default() };
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(count.load(Ordering::SeqCst) > 0);
        drop(capture); // cancelled lifecycle
        std::thread::sleep(std::time::Duration::from_millis(20));
        let after = count.load(Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(count.load(Ordering::SeqCst), after, "timer kept running after drop");
        assert_eq!(topic.snapshot().value, SpectrumState::default(), "running state cleared");
    }

    /// lap.media.compact.idle / lap.media.expand at the provider: a playing player alone
    /// never starts capture; only a presenting subscriber does, and dropping it stops it.
    #[test]
    fn capture_needs_a_presenting_subscriber_not_just_playing_media() {
        fn fake(topic: &Topic<SpectrumState>, _quit: pw::channel::Receiver<()>) -> Result<(), pw::Error> {
            topic.publish(SpectrumState { running: true, values: vec![50.; 16] });
            Ok(())
        }
        let wait = |what: &str, f: &dyn Fn() -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !f() { assert!(std::time::Instant::now() < deadline, "timed out: {what}"); std::thread::sleep(std::time::Duration::from_millis(5)) }
        };
        let runtime = Arc::new(hyprv_core::runtime::ServiceRuntime::new().unwrap());
        let ctx = Context::new(Arc::default(), runtime.clone());
        let (playing, gate) = watch::channel(true);
        let handle = spawn_with(&ctx, gate, fake);
        // Compact bar: media plays, nothing presents it.
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(ctx.counters.snapshot().spectrum_starts, 0);
        assert_eq!(handle.topic.snapshot().value, SpectrumState::default());
        // Expanded: the island subscribes.
        let presented = handle.topic.subscribe();
        wait("capture start", &|| handle.topic.snapshot().value.running);
        assert_eq!(ctx.counters.snapshot().spectrum_starts, 1);
        // Collapsed again: capture stops and values clear.
        drop(presented);
        wait("capture stop", &|| handle.topic.snapshot().value == SpectrumState::default());
        // Presented but paused: still nothing.
        let _presented = handle.topic.subscribe();
        playing.send_replace(false);
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(ctx.counters.snapshot().spectrum_starts, 1);
        runtime.request_shutdown();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn closing_demand_stops_an_active_capture_through_run_sampled() {
        let count = Arc::new(AtomicUsize::new(0));
        let topic = Topic::new(SpectrumState::default());
        let (gate_tx, gate) = watch::channel(true);
        let (stop_tx, stop) = watch::channel(false);
        let (wanted, driver) = gated_demand(topic.demand(), vec![gate], stop.clone());
        struct Fake { count: Arc<AtomicUsize>, worker: Option<Worker> }
        impl Service for Fake {
            fn start(&mut self) -> ServiceFuture<'_> {
                Box::pin(async move { self.worker = Some(ticking_worker(self.count.clone())); Ok(()) })
            }
            fn stop(&mut self) -> ServiceFuture<'_> {
                Box::pin(async move { if let Some(w) = self.worker.take() { w.stop().await } Ok(()) })
            }
        }
        let sub = topic.subscribe();
        let task = tokio::spawn(async move {
            let _ = tokio::join!(driver, run_sampled(Fake { count: count.clone(), worker: None }, wanted, stop));
            count
        });
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        drop(gate_tx); // gate input closes: driver ends
        let count = task.await.unwrap();
        let after = count.load(Ordering::SeqCst);
        assert!(after > 0);
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        assert_eq!(count.load(Ordering::SeqCst), after, "worker joined, timer gone");
        drop((sub, stop_tx));
    }
}
