//! UI-owned bounded cache. File/theme lookup and rasterization run only on the
//! blocking pool; completion carries pixels, never a GPU/Skia object.
use super::*;
use hyprv_core::{calloop::channel::Sender, runtime::ServiceRuntime, tokio};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Sizes {
    pub request: u32,
    pub theme_scale: u32,
    pixmap_target: u32,
}
impl Sizes {
    pub fn new(dpr: f32) -> Option<Self> {
        if !dpr.is_finite() || dpr <= 0. || dpr > 8. {
            return None;
        }
        let request = (18. * dpr).round().max(1.) as u32;
        Some(Self {
            request,
            theme_scale: dpr.ceil() as u32,
            pixmap_target: (18. * dpr).ceil() as u32,
        })
    }
}
type Key = (String, Sizes);
struct Entry {
    item: TrayItem,
    image: Option<sk::Image>,
    token: u64,
    bytes: usize,
}
#[derive(Default)]
struct State {
    entries: HashMap<Key, Entry>,
    fallbacks: HashMap<u32, sk::Image>,
    inflight: HashSet<u64>,
    next: u64,
    bytes: usize,
}
#[derive(Default)]
pub struct TrayLoader {
    state: RefCell<State>,
}
pub struct TrayReady {
    key: Key,
    token: u64,
    pixels: Option<Pixels>,
}
struct Pixels {
    width: i32,
    height: i32,
    bytes: Vec<u8>,
}
const CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_JOBS: usize = 2;

fn fallback(state: &mut State, request: u32) -> sk::Image {
    // Sizes admits at most 144 request sizes; reuse their identity even when
    // jobs/cache are saturated so placeholders cannot keep damaging the scene.
    state
        .fallbacks
        .entry(request)
        .or_insert_with(|| missing_icon(request))
        .clone()
}

impl TrayLoader {
    pub fn retain(&self, items: &[TrayItem], scales: impl Iterator<Item = f32>) {
        let sizes: Vec<_> = scales.filter_map(Sizes::new).collect();
        let mut state = self.state.borrow_mut();
        state.entries.retain(|(key, size), _| {
            sizes.contains(size) && items.iter().any(|item| &item.key == key)
        });
        state.bytes = state.entries.values().map(|entry| entry.bytes).sum();
        state
            .fallbacks
            .retain(|request, _| !items.is_empty() && sizes.iter().any(|s| s.request == *request));
        // In-flight jobs retain their slots until their bounded work completes;
        // absent entries make those replies harmless, rather than spawning duplicates.
    }
    pub fn get(
        &self,
        item: &TrayItem,
        dpr: f32,
        runtime: &ServiceRuntime,
        sender: &Sender<Ready>,
    ) -> Option<sk::Image> {
        let sizes = Sizes::new(dpr)?;
        let key = (item.key.clone(), sizes);
        let mut state = self.state.borrow_mut();
        if let Some(entry) = state.entries.get(&key) {
            if same_tray_icon(&entry.item, item) {
                return entry.image.clone();
            }
        }
        // Bound retained/request metadata, including supplied pixmap arrays.
        let input_bytes = icon_input_bytes(item)?;
        let direct = tray_pixmap(item, dpr);
        let name = if item.status == "NeedsAttention" && !item.attention_icon_name.is_empty() {
            &item.attention_icon_name
        } else {
            &item.icon_name
        };
        let needs_file = direct.is_none() && !name.is_empty();
        if needs_file && state.inflight.len() >= MAX_JOBS {
            // A completion triggers a refresh and retries deferred requests.
            return state
                .entries
                .get(&key)
                .and_then(|e| e.image.clone())
                .or_else(|| Some(fallback(&mut state, sizes.request)));
        }
        let image = direct.or_else(|| needs_file.then(|| fallback(&mut state, sizes.request)));
        let bytes = input_bytes
            + image
                .as_ref()
                .map_or(0, |i| i.width() as usize * i.height() as usize * 4);
        if bytes > CACHE_BYTES {
            return image;
        }
        let old_bytes = state.entries.get(&key).map_or(0, |e| e.bytes);
        if (!state.entries.contains_key(&key) && state.entries.len() >= 128)
            || state.bytes - old_bytes + bytes > CACHE_BYTES
        {
            // Saturation must not evict/reschedule icons on every completion:
            // that would create an idle redraw/loader loop with many tray items.
            return image;
        }
        if let Some(old) = state.entries.remove(&key) {
            state.bytes -= old.bytes;
        }
        state.next = state
            .next
            .checked_add(1)
            .expect("tray request generation exhausted");
        let token = state.next;
        // Don't retain unrelated menu/tooltip data or send it to the worker.
        let identity = icon_identity(item);
        state.bytes += bytes;
        state.entries.insert(
            key.clone(),
            Entry {
                item: identity,
                image: image.clone(),
                token,
                bytes,
            },
        );
        if needs_file {
            state.inflight.insert(token);
            let job = TrayItem {
                icon_name: name.clone(),
                icon_theme_path: item.icon_theme_path.clone(),
                ..Default::default()
            };
            let sender = sender.clone();
            runtime.spawn(async move {
                let pixels = tokio::task::spawn_blocking(move || {
                    themed_icon(&job, sizes).and_then(|image| {
                        let (width, height) = (image.width(), image.height());
                        if !valid_dimensions(width, height) {
                            return None;
                        }
                        let info = sk::ImageInfo::new(
                            (width, height),
                            sk::ColorType::RGBA8888,
                            sk::AlphaType::Premul,
                            None,
                        );
                        let mut bytes = vec![0; width as usize * height as usize * 4];
                        image
                            .read_pixels(
                                &info,
                                &mut bytes,
                                width as usize * 4,
                                (0, 0),
                                sk::image::CachingHint::Disallow,
                            )
                            .then_some(Pixels {
                                width,
                                height,
                                bytes,
                            })
                    })
                })
                .await
                .unwrap_or_default();
                let _ = sender.send(Ready::Tray(TrayReady { key, token, pixels }));
            });
        }
        image
    }
    pub fn complete(&self, ready: TrayReady) {
        let mut state = self.state.borrow_mut();
        state.inflight.remove(&ready.token);
        if state
            .entries
            .get(&ready.key)
            .is_none_or(|e| e.token != ready.token)
        {
            return;
        }
        if let Some(p) = ready.pixels {
            let added = p.bytes.len();
            let entry = state.entries.get(&ready.key).unwrap();
            let previous = entry
                .image
                .as_ref()
                .map_or(0, |i| i.width() as usize * i.height() as usize * 4);
            if state.bytes - previous + added > CACHE_BYTES {
                // Keep the bounded placeholder; never retry this generation in a loop.
                return;
            }
            let info = sk::ImageInfo::new(
                (p.width, p.height),
                sk::ColorType::RGBA8888,
                sk::AlphaType::Premul,
                None,
            );
            let image = sk::images::raster_from_data(
                &info,
                sk::Data::new_copy(&p.bytes),
                p.width as usize * 4,
            );
            if let Some(image) = image {
                let entry = state.entries.get_mut(&ready.key).unwrap();
                entry.image = Some(image);
                entry.bytes = entry.bytes - previous + added;
                state.bytes = state.bytes - previous + added;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyprv_core::calloop::channel::{channel, Channel};
    fn receive(rx: &Channel<Ready>) -> TrayReady {
        let end = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            match rx.try_recv() {
                Ok(Ready::Tray(ready)) => return ready,
                Ok(_) => panic!("unexpected art completion"),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    assert!(std::time::Instant::now() < end, "tray worker stalled");
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(e) => panic!("tray channel closed: {e}"),
            }
        }
    }
    fn svg(tag: &str, color: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "hyprv-tray-loader-{tag}-{}.svg",
            std::process::id()
        ));
        std::fs::write(&p, format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\"><rect width=\"100\" height=\"100\" fill=\"{color}\"/></svg>")).unwrap();
        p
    }
    #[test]
    fn asynchronous_dpr_transition_matches_fresh_and_metadata_keeps_identity() {
        let p = svg("dpr", "red");
        let mut item = TrayItem {
            key: "dpr".into(),
            icon_name: p.display().to_string(),
            ..Default::default()
        };
        let loader = TrayLoader::default();
        let runtime = ServiceRuntime::new().unwrap();
        let (tx, rx) = channel();
        loader.get(&item, 1.25, &runtime, &tx);
        assert_eq!(loader.state.borrow().inflight.len(), 1);
        loader.complete(receive(&rx));
        let first = loader.get(&item, 1.25, &runtime, &tx).unwrap();
        assert_eq!((first.width(), first.height()), (46, 46));
        item.title = "metadata update".into();
        assert_eq!(
            loader.get(&item, 1.25, &runtime, &tx).unwrap().unique_id(),
            first.unique_id()
        );
        assert!(loader.state.borrow().inflight.is_empty());
        loader.get(&item, 1.3, &runtime, &tx);
        loader.complete(receive(&rx));
        let changed = loader.get(&item, 1.3, &runtime, &tx).unwrap();
        item.key = "fresh".into();
        loader.get(&item, 1.3, &runtime, &tx);
        loader.complete(receive(&rx));
        let fresh = loader.get(&item, 1.3, &runtime, &tx).unwrap();
        assert_eq!((changed.width(), changed.height()), (46, 46));
        assert_eq!(changed.dimensions(), fresh.dimensions());
        assert_ne!(
            Sizes::new(1.).unwrap(),
            Sizes::new(1.01).unwrap(),
            "supplied-pixmap ceil(DPR) target remains part of the key"
        );
        assert!(loader.state.borrow().inflight.is_empty());
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn asynchronous_theme_provider_matches_qicon_default_screen_dpr() {
        let p = svg("device-pixels", "red");
        let item = TrayItem {
            key: "device-pixels".into(),
            icon_name: p.display().to_string(),
            ..Default::default()
        };
        let loader = TrayLoader::default();
        let runtime = ServiceRuntime::new().unwrap();
        let (tx, rx) = channel();
        loader.get(&item, 2., &runtime, &tx);
        loader.complete(receive(&rx));
        let image = loader.get(&item, 2., &runtime, &tx).unwrap();
        assert_eq!((image.width(), image.height()), (72, 72));
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn late_completion_cannot_replace_newer_icon() {
        let red = svg("old", "red");
        let blue = svg("new", "blue");
        let mut item = TrayItem {
            key: "changing".into(),
            icon_name: red.display().to_string(),
            ..Default::default()
        };
        let loader = TrayLoader::default();
        let runtime = ServiceRuntime::new().unwrap();
        let (tx, rx) = channel();
        loader.get(&item, 1., &runtime, &tx);
        item.icon_name = blue.display().to_string();
        loader.get(&item, 1., &runtime, &tx);
        let mut ready = [receive(&rx), receive(&rx)];
        ready.sort_by_key(|r| std::cmp::Reverse(r.token));
        for r in ready {
            loader.complete(r);
        }
        let image = loader.get(&item, 1., &runtime, &tx).unwrap();
        let info = sk::ImageInfo::new((1, 1), sk::ColorType::RGBA8888, sk::AlphaType::Premul, None);
        let mut pixel = [0u8; 4];
        assert!(image.read_pixels(
            &info,
            &mut pixel,
            4,
            (5, 5),
            sk::image::CachingHint::Disallow
        ));
        assert_eq!(pixel, [0, 0, 255, 255]);
        assert!(loader.state.borrow().inflight.is_empty());
        std::fs::remove_file(red).unwrap();
        std::fs::remove_file(blue).unwrap();
    }
    #[test]
    fn saturated_cache_does_not_create_a_reload_loop() {
        let loader = TrayLoader::default();
        let runtime = ServiceRuntime::new().unwrap();
        let (tx, rx) = channel();
        for n in 0..128 {
            loader.get(
                &TrayItem {
                    key: n.to_string(),
                    ..Default::default()
                },
                1.,
                &runtime,
                &tx,
            );
        }
        let p = svg("capacity", "red");
        let item = TrayItem {
            key: "overflow".into(),
            icon_name: p.display().to_string(),
            ..Default::default()
        };
        let first = loader.get(&item, 1., &runtime, &tx).unwrap();
        for _ in 0..3 {
            assert_eq!(
                loader.get(&item, 1., &runtime, &tx).unwrap().unique_id(),
                first.unique_id()
            );
        }
        assert_eq!(loader.state.borrow().entries.len(), 128);
        assert!(loader.state.borrow().inflight.is_empty());
        assert!(rx.try_recv().is_err());
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn concurrent_file_jobs_are_bounded_and_deferred_requests_retry() {
        let loader = TrayLoader::default();
        let runtime = ServiceRuntime::new().unwrap();
        let (tx, rx) = channel();
        loader.state.borrow_mut().inflight.extend([1000, 1001]);
        let p = svg("job-bound", "red");
        let item = TrayItem {
            key: "deferred".into(),
            icon_name: p.display().to_string(),
            ..Default::default()
        };
        assert!(
            loader.get(&item, 1., &runtime, &tx).is_some(),
            "deferred nonempty icon still draws the checker"
        );
        assert!(loader.state.borrow().entries.is_empty());
        loader.state.borrow_mut().inflight.remove(&1000);
        loader.get(&item, 1., &runtime, &tx);
        assert_eq!(loader.state.borrow().inflight.len(), MAX_JOBS);
        loader.complete(receive(&rx));
        assert_eq!(loader.state.borrow().inflight.len(), 1);
        std::fs::remove_file(p).unwrap();
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn output_and_item_removal_release_cache_entries() {
        let loader = TrayLoader::default();
        let runtime = ServiceRuntime::new().unwrap();
        let (tx, _rx) = hyprv_core::calloop::channel::channel();
        let item = TrayItem {
            key: "retained".into(),
            ..Default::default()
        };
        loader.get(&item, 1., &runtime, &tx);
        loader.get(&item, 2., &runtime, &tx);
        assert_eq!(loader.state.borrow().entries.len(), 2);
        loader.retain(std::slice::from_ref(&item), [2.].into_iter());
        assert_eq!(loader.state.borrow().entries.len(), 1);
        loader.retain(&[], [2.].into_iter());
        assert!(loader.state.borrow().entries.is_empty());
        assert_eq!(loader.state.borrow().bytes, 0);
    }
}
