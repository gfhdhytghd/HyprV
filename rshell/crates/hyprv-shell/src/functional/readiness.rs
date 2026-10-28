//! Completion regressions use the production Ready channel and live Adapter.
//! This module is reachable only through the private mocked functional harness.
use super::*;
use hyprv_svc_media::tray::TrayItem;

enum Probe {
    Art,
    Tray {
        item: Box<TrayItem>,
        placeholder: u32,
    },
}
thread_local! { static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) }; }

pub(super) fn queue(
    a: &mut Assembly,
    args: &[String],
) -> Result<Value, Box<dyn std::error::Error>> {
    let kind = args.first().ok_or("asset-ready needs art or tray")?;
    let mut surface = hyprv_ui::skia::surfaces::raster_n32_premul((2, 2)).ok_or("test image")?;
    surface.canvas().clear(hyprv_ui::skia::Color::RED);
    let bytes = surface
        .image_snapshot()
        .encode(None, hyprv_ui::skia::EncodedImageFormat::PNG, None)
        .ok_or("test PNG")?
        .as_bytes()
        .to_vec();
    match kind.as_str() {
        "art" => {
            a.art = None;
            // Mocked fixtures suppress external art fetches; use the current URL
            // to take the same matching-generation branch as a real completion.
            let url = a.art_url.clone();
            PROBE.with(|p| *p.borrow_mut() = Some(Probe::Art));
            a.art_ready
                .send(crate::assets::Ready::Art(crate::assets::Art { url, bytes }))?;
        }
        "tray" => {
            let path = std::path::PathBuf::from(
                std::env::var_os("XDG_RUNTIME_DIR").ok_or("private runtime")?,
            )
            .join("callback-regression-tray.png");
            std::fs::write(&path, bytes)?;
            let item = TrayItem {
                key: "callback-regression".into(),
                icon_name: path.to_string_lossy().into_owned(),
                ..Default::default()
            };
            // A real bounded worker resolves/rasterizes this file and sends
            // Ready::Tray. No synthetic complete() call or fake Engine is used.
            let placeholder = a
                .tray_loader
                .get(&item, 1., &a.hub.runtime, &a.art_ready)
                .ok_or("tray placeholder")?
                .unique_id();
            PROBE.with(|p| {
                *p.borrow_mut() = Some(Probe::Tray {
                    item: Box::new(item),
                    placeholder,
                })
            });
        }
        _ => return Err("unknown completion kind".into()),
    }
    Ok(json!({"queued":kind,"channel":"production art_receive","handler":"live Assembly Adapter"}))
}

pub(super) fn status(a: &Assembly) -> Result<Value, Box<dyn std::error::Error>> {
    PROBE.with(|probe| match probe.borrow().as_ref().ok_or("no asset probe")? {
        Probe::Art => Ok(json!({"ready":a.art.as_ref().is_some_and(|i|i.width()==2&&i.height()==2),"dirty":a.dirty})),
        Probe::Tray { item, placeholder } => {
            let image = a.tray_loader.get(item, 1., &a.hub.runtime, &a.art_ready);
            Ok(json!({"ready":image.as_ref().is_some_and(|i|i.unique_id()!=*placeholder),"dirty":a.dirty,"placeholder":placeholder,"image":image.map(|i|i.unique_id())}))
        }
    })
}

pub(super) fn run(s: &mut suite::Suite) {
    if std::env::var("HYPRV_CALLBACK_NEGATIVE_CONTROL").as_deref() == Ok("1") {
        // Run in a separate private child: this deliberately reproduces the old
        // held-borrow scheduling order. The runner requires the RefCell panic.
        s.case(
            "callbacks.held-borrow-control",
            "live-handler negative control",
            |s| s.rpc("asset-held-borrow-control", None),
        );
        return;
    }
    for kind in ["art", "tray"] {
        s.case(
            &format!("callbacks.{kind}.ready"),
            "assembly.rs Ready source / synchronous Adapter::update",
            |s| {
                s.reset_seed(Some("idle"))?;
                s.settle(200);
                s.rpc("asset-ready", Some(json!(kind)))?;
                let deadline = std::time::Instant::now() + Duration::from_secs(8);
                loop {
                    let status = s.rpc("asset-status", None)?;
                    if status["ready"] == true {
                        if status["dirty"] != false {
                            return Err(
                                "completion did not reconcile through the live handler".into()
                            );
                        }
                        return Ok(status);
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(format!("{kind} Ready was not applied").into());
                    }
                    s.settle(20);
                }
            },
        );
    }
}
