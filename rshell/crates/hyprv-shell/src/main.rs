use hyprv_core::{
    calloop::{
        channel::{self, Event},
        timer::{TimeoutAction, Timer},
        EventLoop,
    },
    clock::{self, ClockEvent},
    config::{self, Config, Theme},
    ipc::{self, IpcServer, Request, UiRequest},
    runtime::ServiceRuntime,
    store::Topic,
    tokio,
};
use hyprv_shell::{
    host::{Host, HostAdapter, Output},
    theme,
};
use hyprv_ui::wayland::{Application, Engine};
use std::{
    cell::RefCell,
    error::Error,
    rc::Rc,
    time::{Duration, Instant},
};
fn main() {
    hyprv_shell::allocator::initialize();
    if let Err(error) = run() {
        eprintln!("hyprv-shell: {error}");
        std::process::exit(1);
    }
}
#[derive(Default)]
struct Options {
    headless: bool,
    read_only: bool,
    demo: bool,
    no_exclusive: bool,
    exit_after: Option<Duration>,
}
fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = hyprv_shell::compat::normalize_args(args)?;
    let profile = hyprv_core::profile::profile();
    if args == ["--profile-status"] {
        println!("{}", serde_json::json!({"profile":match profile {hyprv_core::profile::Profile::Desktop=>"desktop",hyprv_core::profile::Profile::Laptop=>"laptop"},"sys_root":hyprv_core::profile::sys_root()}));
        return Ok(());
    }
    if args == ["--check-assets"] {
        let root = hyprv_shell::runtime_assets::installed()?;
        println!("Runtime assets verified: {}", root.display());
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|a| matches!(a.as_str(), "--nested-functional-test" | "--nested-idle-test"))
    {
        let directory = std::path::PathBuf::from(
            args.get(1)
                .cloned()
                .unwrap_or_else(|| if args[0] == "--nested-idle-test" {
                    "target/idle-test".into()
                } else {
                    "target/functional-test".into()
                }),
        );
        std::env::set_var("HYPRV_FUNCTIONAL_TEST", "1");
        if args[0] == "--nested-idle-test" {
            std::env::set_var("HYPRV_IDLE_TEST", "1");
            std::env::set_var("HYPRV_UI_PROFILE", "1");
            std::env::set_var("HYPRV_FUNCTIONAL_FILTER", "idle.");
        }
        std::env::set_var(
            "HYPRV_NESTED_FIXTURE",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/assembled-desktop.json"),
        );
        std::env::set_var("HYPRV_NESTED_THEME", "dark");
        if profile == hyprv_core::profile::Profile::Desktop || std::env::var_os("HYPRV_NESTED_SCALE").is_none() { std::env::set_var("HYPRV_NESTED_SCALE", if profile == hyprv_core::profile::Profile::Laptop { "2" } else { "1" }); }
        if profile == hyprv_core::profile::Profile::Desktop || std::env::var_os("HYPRV_NESTED_WIDTH").is_none() { std::env::set_var("HYPRV_NESTED_WIDTH", if profile == hyprv_core::profile::Profile::Laptop { "1440" } else { "3072" }); }
        if profile == hyprv_core::profile::Profile::Desktop || std::env::var_os("HYPRV_NESTED_HEIGHT").is_none() { std::env::set_var("HYPRV_NESTED_HEIGHT", if profile == hyprv_core::profile::Profile::Laptop { "900" } else { "1728" }); }
        return hyprv_shell::nested::run(directory);
    }
    if args.first().is_some_and(|a| a == "--nested-test") {
        let directory = args
            .get(1)
            .filter(|a| !a.starts_with("--"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "target/nested-test".into());
        if args.iter().any(|a| a == "--fixture" || a == "--fixtures") {
            let value = |flag: &str| {
                args.iter()
                    .position(|a| a == flag)
                    .and_then(|i| args.get(i + 1))
                    .cloned()
            };
            let mut fixtures = Vec::new();
            if let Some(file) = value("--fixture") {
                fixtures.push(file.into());
            }
            if let Some(dir) = value("--fixtures") {
                for entry in std::fs::read_dir(dir)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|e| e == "json") {
                        let metadata: serde_json::Value =
                            serde_json::from_slice(&std::fs::read(&path)?)?;
                        if metadata["scene"].is_string() {
                            fixtures.push(path);
                        }
                    }
                }
                fixtures.sort();
            }
            return hyprv_shell::nested::fixture_matrix(
                directory,
                fixtures,
                value("--theme"),
                value("--scale").map(|s| s.parse()).transpose()?,
                value("--references").map(Into::into),
                args.iter().any(|a| a == "--capture-only"),
            );
        }
        return hyprv_shell::nested::run(directory);
    }
    if args.first().is_some_and(|a| a == "desktop") {
        // Patched hypr scripts: running shell via IPC, else the same action in-process.
        return hyprv_shell::desktop_cli::run(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "startup-confirm") {
        // One-shot dialog for hypr/scripts/restore-session-layout.sh (replaces qs StartupConfirm.qml).
        if !hyprv_shell::startup_confirm::run()? {
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "--nested-driver") {
        return hyprv_shell::nested::driver(std::path::Path::new(
            args.get(1).ok_or("missing dump directory")?,
        ));
    }
    if args.first().is_some_and(|a| a == "--nested-child") {
        if std::env::var("HYPRV_NESTED_TEST").as_deref() != Ok("1") {
            return Err("nested-child requires the isolated harness".into());
        }
        std::env::set_var(
            "HYPRV_SMOKE_SCENE",
            args.get(2).ok_or("missing smoke scene")?,
        );
        return hyprv_shell::assembly::run(
            profile == hyprv_core::profile::Profile::Desktop,
            Some(Duration::from_millis(1600)),
            Some(args.get(1).ok_or("missing dump directory")?.into()),
            true,
        );
    }
    if args == ["--help"] || args == ["-h"] {
        println!("hyprv-shell --check-assets\nhyprv-shell --nested-idle-test [REPORT_DIR]\nhyprv-shell --nested-functional-test [REPORT_DIR]\nhyprv-shell --nested-test [DUMP_DIR] [--fixture FILE | --fixtures DIR] [--theme light|dark] [--scale DPR] [--references DIR] [--capture-only]\nhyprv-shell [--demo-host] [--no-exclusive] [--read-only] [--exit-after-ms N] [--headless]\nhyprv-shell ipc call <target> <function> [args..]\nhyprv-shell kill\nhyprv-shell startup-confirm   # one-shot dialog; decision written to $HYPRV_STARTUP_APPROVAL\nhyprv-shell desktop setVariant <v1|v2|v3|bt> | deskflowReflow   # running shell, else in-process");
        return Ok(());
    }
    if args.first().is_some_and(|a| a == "ipc" || a == "kill") {
        let request = if args == ["kill"] {
            Request {
                target: "shell".into(),
                function: "kill".into(),
                args: Vec::new(),
            }
        } else if args.len() >= 4 && args[0] == "ipc" && args[1] == "call" {
            Request {
                target: args[2].clone(),
                function: args[3].clone(),
                args: args[4..].to_vec(),
            }
        } else {
            return Err("usage: hyprv-shell ipc call <target> <function> [args..] | kill".into());
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let response = runtime.block_on(ipc::call(&ipc::socket_path()?, &request))?;
        if !response.ok {
            return Err(response.result.into());
        }
        if !response.result.is_empty() {
            println!("{}", response.result);
        }
        return Ok(());
    }
    let mut options = Options::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--headless" => options.headless = true,
            "--read-only" => options.read_only = true,
            "--demo-host" => options.demo = true,
            "--no-exclusive" => options.no_exclusive = true,
            "--exit-after-ms" => {
                i += 1;
                let ms: u64 = args.get(i).ok_or("missing exit duration")?.parse()?;
                if ms == 0 {
                    return Err("exit duration must be positive".into());
                }
                options.exit_after = Some(Duration::from_millis(ms));
            }
            _ => return Err(format!("unknown option {}", args[i]).into()),
        }
        i += 1;
    }
    serve(options)
}
fn serve(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.headless {
        return hyprv_shell::assembly::run(
            options.no_exclusive,
            options.exit_after,
            None,
            options.read_only,
        );
    }
    let runtime = ServiceRuntime::new()?;
    let server = runtime.enter(|| IpcServer::bind(ipc::socket_path()?))?;
    let host = Rc::new(RefCell::new(Host::new(options.no_exclusive, options.demo)));
    host.borrow_mut()
        .set_dark(theme::read_dark(&theme::path()?));
    let (ui_send, ui_receive) = ServiceRuntime::bridge::<UiRequest>();
    let server_shutdown = runtime.shutdown_signal();
    let (exit_send, exit_receive) = ServiceRuntime::bridge::<()>();
    let server_task = runtime.spawn(async move {
        let result = server.run(ipc::ui_handler(ui_send), server_shutdown).await;
        let _ = exit_send.send(());
        result
    });
    let result = if options.headless {
        host.borrow_mut().add_output(Output {
            name: "HEADLESS-TEST".into(),
            width: 1280.,
            height: 720.,
        });
        let epoch = Instant::now();
        let mut event_loop = EventLoop::<bool>::try_new()?;
        let state = host.clone();
        event_loop
            .handle()
            .insert_source(ui_receive, move |event, _, quit| {
                if let Event::Msg(message) = event {
                    let mut state = state.borrow_mut();
                    state.tick(epoch.elapsed());
                    let response = state.dispatch(message.request, epoch.elapsed());
                    let _ = message.reply.send(response);
                    *quit = state.quit;
                }
            })?;
        event_loop
            .handle()
            .insert_source(exit_receive, |event, _, quit| {
                if matches!(event, Event::Msg(())) {
                    *quit = true;
                }
            })?;
        if let Some(duration) = options.exit_after {
            event_loop
                .handle()
                .insert_source(Timer::from_duration(duration), |_, _, quit| {
                    *quit = true;
                    TimeoutAction::Drop
                })?;
        }
        let mut quit = false;
        while !quit {
            event_loop.dispatch(None, &mut quit)?;
        }
        Ok(())
    } else {
        let mut app = Application::connect(HostAdapter(host.clone()))?;
        let handle = app.event_loop_handle();
        let state = host.clone();
        handle.insert_source(ui_receive, move |event, _, engine| {
            if let Event::Msg(message) = event {
                let response = state.borrow_mut().dispatch(message.request, engine.now());
                let _ = message.reply.send(response);
                engine.request_redraw();
            }
        })?;
        handle.insert_source(exit_receive, |event, _, engine| {
            if matches!(event, Event::Msg(())) {
                engine.quit();
            }
        })?;
        if let Some(duration) = options.exit_after {
            handle.insert_source(Timer::from_duration(duration), |_, _, engine| {
                engine.quit();
                TimeoutAction::Drop
            })?;
        }
        attach_services(&runtime, &host, &mut app)?;
        app.run().map_err(|e| Box::new(e) as Box<dyn Error>)
    };
    runtime.request_shutdown();
    runtime.block_on(server_task)??;
    result
}
fn attach_services(
    runtime: &ServiceRuntime,
    host: &Rc<RefCell<Host>>,
    app: &mut Application,
) -> Result<(), Box<dyn Error>> {
    let path = Config::path()?;
    let initial = Config::load(&path)?;
    let colors = Theme::default()
        .reload(&initial.theme_file)
        .unwrap_or_default();
    host.borrow_mut().matugen = colors.clone();
    let configuration = Topic::new(initial);
    let colors = Topic::new(colors);
    let shutdown = runtime.shutdown_signal();
    let theme_topic = colors.clone();
    runtime.spawn(async move {
        if let Err(e) = config::watch_config(path, configuration, theme_topic, shutdown).await {
            eprintln!("config watch: {e}");
        }
    });
    let (theme_send, theme_receive) = channel::channel::<bool>();
    let shutdown = runtime.shutdown_signal();
    let theme_path = theme::path()?;
    runtime.spawn(async move {
        if let Err(e) = theme::watch(theme_path, theme_send, shutdown).await {
            eprintln!("theme watch: {e}");
        }
    });
    let state = host.clone();
    app.event_loop_handle()
        .insert_source(theme_receive, move |event, _, engine| {
            if let Event::Msg(dark) = event {
                state.borrow_mut().set_dark(dark);
                engine.request_redraw();
            }
        })?;
    let (color_send, color_receive) = channel::channel::<Theme>();
    let mut subscription = colors.subscribe();
    let mut shutdown = runtime.shutdown_signal();
    runtime.spawn(async move{loop{tokio::select!{value=subscription.changed()=>{let Ok(value)=value else{return;};if color_send.send(value.value).is_err(){return;}},_=shutdown.changed()=>return}}});
    let state = host.clone();
    app.event_loop_handle()
        .insert_source(color_receive, move |event, _, engine| {
            if let Event::Msg(theme) = event {
                let mut state = state.borrow_mut();
                state.matugen = theme;
                state.invalidate();
                drop(state);
                engine.request_redraw();
            }
        })?;
    let time = Topic::new(clock::unix_seconds() / 60);
    host.borrow_mut().attach_clock(time.clone());
    let (clock_send, clock_receive) = channel::channel::<ClockEvent>();
    let shutdown = runtime.shutdown_signal();
    let topic = time.clone();
    let sender = clock_send.clone();
    runtime.spawn(async move {
        if let Err(e) = clock::run_clock(topic, sender, shutdown).await {
            eprintln!("clock: {e}");
        }
    });
    let shutdown = runtime.shutdown_signal();
    runtime.spawn(async move {
        if let Err(e) = clock::logind_sleep(clock_send, shutdown).await {
            eprintln!("logind: {e}");
        }
    });
    let state = host.clone();
    app.event_loop_handle()
        .insert_source(clock_receive, move |event, _, engine| {
            if let Event::Msg(event) = event {
                let minute = match event {
                    ClockEvent::Minute(s) | ClockEvent::ClockChanged(s) => Some(s / 60),
                    ClockEvent::PrepareForSleep(false) => Some(clock::unix_seconds() / 60),
                    _ => None,
                };
                if let Some(minute) = minute {
                    time.publish(minute);
                    let mut state = state.borrow_mut();
                    if state.minute != minute {
                        state.minute = minute;
                        state.invalidate();
                    }
                    drop(state);
                    engine.request_redraw();
                }
            }
        })?;
    let (signal_send, signal_receive) = channel::channel::<()>();
    let mut shutdown = runtime.shutdown_signal();
    runtime.spawn(async move{use tokio::signal::unix::{signal,SignalKind};let mut term=signal(SignalKind::terminate()).expect("SIGTERM");tokio::select!{_=tokio::signal::ctrl_c()=>(),_=term.recv()=>(),_=shutdown.changed()=>return}let _=signal_send.send(());});
    app.event_loop_handle()
        .insert_source(signal_receive, |event, _, engine: &mut Engine| {
            if matches!(event, Event::Msg(())) {
                engine.quit();
            }
        })?;
    Ok(())
}
