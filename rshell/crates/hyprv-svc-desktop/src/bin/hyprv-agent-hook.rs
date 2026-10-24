//! Drop-in replacement for `agent-island-hook.py` (same CLI and state file):
//!   hyprv-agent-hook --source claude|codex [--event NAME]   < hook JSON on stdin
//!   hyprv-agent-hook install [--python]                     (explicit maintenance)
//! This process is spawned by the agent CLI, never by the shell.
use hyprv_svc_desktop::{
    agent_island::{install, model, store},
    hypr::Hypr,
    util, Env,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

fn active_window(env: &Env) -> Option<Value> {
    let dir = Hypr::new(env).instance_dir()?;
    let mut s = UnixStream::connect(dir.join(".socket.sock")).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(400))).ok()?;
    s.set_write_timeout(Some(Duration::from_millis(400))).ok()?;
    s.write_all(b"j/activewindow").ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    model::window_snapshot(&serde_json::from_str(out.trim()).ok()?)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = Env::from_process();
    if args.first().map(String::as_str) == Some("install") {
        let cmd = if args.iter().any(|a| a == "--python") {
            install::HookCommand::legacy(&env)
        } else {
            install::HookCommand::Native {
                binary: std::env::current_exe().unwrap_or_else(|_| "hyprv-agent-hook".into()),
            }
        };
        match install::install(&env, &cmd) {
            Ok(out) => print!("{out}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let mut source = None;
    let mut event_arg = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--source" => source = it.next().cloned(),
            "--event" => event_arg = it.next().cloned(),
            s if s.starts_with("--source=") => source = Some(s[9..].into()),
            s if s.starts_with("--event=") => event_arg = Some(s[8..].into()),
            _ => {}
        }
    }
    if source.is_none() {
        eprintln!("usage: hyprv-agent-hook --source SOURCE [--event EVENT]");
        std::process::exit(2);
    }
    // Every exit below is 0: a hook must never disturb the agent.
    let Some(source) = model::normalize_source(source.as_deref()) else {
        return;
    };
    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    let Ok(Value::Object(mut payload)) = serde_json::from_slice::<Value>(&raw) else {
        return;
    };
    if payload.is_empty() {
        return;
    }
    let event =
        model::normalize_event(model::event_name(&payload, event_arg.as_deref()).as_deref());
    if event.is_empty() {
        return;
    }
    let ppid = unsafe { libc::getppid() } as i64;
    payload.insert("_source".into(), source.into());
    payload.entry("_ppid").or_insert(ppid.into());
    let pid = model::int_value(payload.get("_ppid"))
        .filter(|p| *p != 0)
        .unwrap_or(ppid);
    let vars: HashMap<String, String> = std::env::vars().collect();
    let terminal = model::terminal_meta(&vars, active_window(&env), pid);
    let dir = store::state_dir(&env);
    let now = util::epoch_f64();
    let proc = util::Proc::default();
    let _ = store::with_locked(&dir, now, |state| {
        model::cleanup_state(state, now, &mut |p| proc.exists(p as i32));
        let (sid, rid) =
            model::update_session_from_event(state, &payload, source, &event, terminal, ppid, now);
        // Observe permission/input state; the agent's own UI handles responses.
        if let Some(rid) = rid {
            model::drop_request(state, &sid, &rid);
        }
        (true, ())
    });
}
