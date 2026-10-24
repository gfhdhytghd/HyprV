//! Application index and activation.
//!
//! Index: port of `hypr-comp/.../app_fetcher.py` (same directories, first
//! `Name=`/`Exec=`/`Icon=` in `[Desktop Entry]`, NoDisplay skipped, deduplicated
//! by name with later directories winning, case-insensitive name sort),
//! refreshed by inotify instead of a process run.
//!
//! Activation never forks: `DBusActivatable=true` entries use
//! `org.freedesktop.Application.Activate`; everything else is started by the
//! systemd user manager as a transient `app-hyprv-*` unit (what `systemd-run`
//! does), with Desktop Entry field codes parsed properly. Fixed shell launch
//! targets (`shell.qml` runDetached calls) are typed, not arbitrary commands.
use crate::{systemd, topic_key, util, ActionResult, Ctx, Env, Handle, ServiceError};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    pub desktop_id: String,
    /// Launcher wire fields `{name, exec, icon}`; `exec` is truncated before ` %`/` @@`.
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub categories: Vec<String>,
    pub terminal: bool,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    raw_exec: String,
    #[serde(skip)]
    dbus_activatable: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationState {
    pub applications: Vec<Application>,
}
topic_key!(ApplicationsTopic, ApplicationState);

/// Launch targets used by the bar/panels (`shell.qml` runDetached sites).
#[derive(Clone, Debug, PartialEq)]
pub enum LaunchTarget {
    /// `rofi-wayland -show drun` (`shell.qml:3780`).
    Launcher,
    /// `hyprlock` (`shell.qml:3942`).
    Lock,
    /// `wlogout --protocol layer-shell -b 5` (`shell.qml:3944`).
    Logout,
    /// `alacritty -t btop -o window.startup_mode=Fullscreen -e btop`.
    Btop,
    /// `pavucontrol` (`shell.qml:4140`).
    Pavucontrol,
    /// nm-connection-editor, else iwgtk, else `alacritty -t nmtui -e nmtui`.
    NetworkEditor,
    /// blueman-manager, else blueberry, else `alacritty -t bluetoothctl -e bluetoothctl`.
    BluetoothManager,
    /// `~/.local/bin/viewflow-windows start --anchor X Y` (`shell.qml:3798`).
    ViewflowWindowsStart { x: i64, y: i64 },
    /// `~/.local/bin/viewflow-windows menu`.
    ViewflowWindowsMenu,
}
#[derive(Clone, Debug)]
pub enum AppAction {
    Activate(String),
    ActivateByName(String),
    Launch(LaunchTarget),
    /// `org.freedesktop.portal.OpenURI.OpenURI`.
    OpenUri(String),
}
pub type ApplicationsHandle = Handle<ApplicationState, AppAction>;

pub fn search_dirs(env: &Env) -> Vec<PathBuf> {
    let h = &env.home;
    vec![
        "/usr/share/applications".into(),
        "/usr/local/share/applications".into(),
        h.join(".local/share/applications"),
        "/var/lib/flatpak/exports/share/applications".into(),
        h.join(".local/share/flatpak/exports/share/applications"),
        h.join(".nix-profile/share/applications"),
        "/run/current-system/sw/share/applications".into(),
    ]
}

/// Parse one desktop file with app_fetcher.py's line rules.
pub fn parse_entry(text: &str, path: &Path, id: String) -> Option<Application> {
    let (mut name, mut exec, mut icon) = (String::new(), String::new(), String::new());
    let (mut in_entry, mut no_display, mut terminal, mut dbus) = (false, false, false, false);
    let mut categories = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "[Desktop Entry]" {
            in_entry = true;
        } else if line.starts_with('[') {
            in_entry = false;
        }
        if !in_entry {
            continue;
        }
        if let Some(v) = line.strip_prefix("Name=") {
            if name.is_empty() {
                name = v.into();
            }
        } else if let Some(v) = line.strip_prefix("Exec=") {
            if exec.is_empty() {
                exec = v.into();
            }
        } else if let Some(v) = line.strip_prefix("Icon=") {
            if icon.is_empty() {
                icon = v.into();
            }
        } else if line.starts_with("NoDisplay=true") || line.starts_with("NoDisplay=1") {
            no_display = true;
        } else if let Some(v) = line.strip_prefix("Terminal=") {
            terminal = v.trim() == "true";
        } else if let Some(v) = line.strip_prefix("DBusActivatable=") {
            dbus = v.trim() == "true";
        } else if let Some(v) = line.strip_prefix("Categories=") {
            categories = v
                .split(';')
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    if name.is_empty() || exec.is_empty() || no_display {
        return None;
    }
    let short = exec
        .split(" %")
        .next()
        .unwrap_or("")
        .split(" @@")
        .next()
        .unwrap_or("")
        .to_string();
    Some(Application {
        desktop_id: id,
        name,
        exec: short,
        icon,
        categories,
        terminal,
        path: path.to_path_buf(),
        raw_exec: exec,
        dbus_activatable: dbus,
    })
}
fn walk(dir: &Path, root: &Path, files: &mut Vec<(PathBuf, String)>, dirs: &mut Vec<PathBuf>) {
    let Ok(it) = std::fs::read_dir(dir) else {
        return;
    };
    dirs.push(dir.to_path_buf());
    let mut entries: Vec<_> = it.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, root, files, dirs);
        } else if p.extension().is_some_and(|e| e == "desktop") {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('/', "-");
            files.push((p, rel));
        }
    }
}
/// Returns the sorted index and every directory to watch.
pub fn index(dirs: &[PathBuf]) -> (Vec<Application>, Vec<PathBuf>) {
    let mut by_name: BTreeMap<String, Application> = BTreeMap::new();
    let mut watched = Vec::new();
    for d in dirs {
        let mut files = Vec::new();
        walk(d, d, &mut files, &mut watched);
        for (path, id) in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse_entry(&text, &path, id) {
                by_name.insert(app.name.clone(), app);
            }
        }
    }
    let mut apps: Vec<Application> = by_name.into_values().collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    (apps, watched)
}

/// Desktop Entry `Exec` tokenizer with field-code expansion (no files/URLs).
pub fn exec_argv(exec: &str, app: Option<&Application>) -> Vec<String> {
    let mut args: Vec<(String, bool)> = Vec::new();
    let mut cur = String::new();
    let (mut in_quote, mut has) = (false, false);
    let mut quoted = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quote = !in_quote;
                has = true;
                quoted = true;
            }
            '\\' if in_quote => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ' ' | '\t' if !in_quote => {
                if has || !cur.is_empty() {
                    args.push((std::mem::take(&mut cur), quoted));
                }
                has = false;
                quoted = false;
            }
            _ => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        args.push((cur, quoted));
    }
    let mut out = Vec::new();
    for (arg, quoted) in args {
        if quoted {
            out.push(arg);
            continue;
        }
        match arg.as_str() {
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            "%i" => {
                if let Some(a) = app.filter(|a| !a.icon.is_empty()) {
                    out.push("--icon".into());
                    out.push(a.icon.clone());
                }
            }
            "%c" => out.extend(app.map(|a| a.name.clone())),
            "%k" => out.extend(app.map(|a| a.path.display().to_string())),
            _ => out.push(arg.replace("%%", "%")),
        }
    }
    out
}

pub fn launch_argv(env: &Env, target: &LaunchTarget) -> Result<Vec<String>, ServiceError> {
    let path = env.var("PATH");
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let first = |cands: &[&str], fallback: &[&str]| {
        cands
            .iter()
            .find(|c| util::which(c, path).is_some())
            .map(|c| vec![c.to_string()])
            .unwrap_or_else(|| s(fallback))
    };
    let viewflow = env
        .home
        .join(".local/bin/viewflow-windows")
        .display()
        .to_string();
    Ok(match target {
        LaunchTarget::Launcher => s(&["rofi-wayland", "-show", "drun"]),
        LaunchTarget::Lock => s(&["hyprlock"]),
        LaunchTarget::Logout => s(&["wlogout", "--protocol", "layer-shell", "-b", "5"]),
        LaunchTarget::Btop => s(&[
            "alacritty",
            "-t",
            "btop",
            "-o",
            "window.startup_mode=Fullscreen",
            "-e",
            "btop",
        ]),
        LaunchTarget::Pavucontrol => s(&["pavucontrol"]),
        LaunchTarget::NetworkEditor => first(
            &["nm-connection-editor", "iwgtk"],
            &["alacritty", "-t", "nmtui", "-e", "nmtui"],
        ),
        LaunchTarget::BluetoothManager => first(
            &["blueman-manager", "blueberry"],
            &["alacritty", "-t", "bluetoothctl", "-e", "bluetoothctl"],
        ),
        LaunchTarget::ViewflowWindowsStart { x, y } => {
            vec![
                viewflow,
                "start".into(),
                "--anchor".into(),
                x.to_string(),
                y.to_string(),
            ]
        }
        LaunchTarget::ViewflowWindowsMenu => vec![viewflow, "menu".into()],
    })
}
/// systemd ExecStart needs an absolute path; resolve through PATH.
fn absolute(env: &Env, mut argv: Vec<String>) -> Result<Vec<String>, ServiceError> {
    let exe = argv
        .first()
        .ok_or_else(|| ServiceError::new("invalid", "empty command"))?;
    if !exe.starts_with('/') {
        let resolved = util::which(exe, env.var("PATH"))
            .ok_or_else(|| ServiceError::new("not_found", format!("{exe}: command not found")))?;
        argv[0] = resolved.display().to_string();
    }
    Ok(argv)
}
async fn start_unit(env: &Env, slug: &str, argv: Vec<String>, description: &str) -> ActionResult {
    let argv = absolute(env, argv)?;
    let mgr = systemd::UserManager::connect().await?;
    let unit = format!(
        "app-hyprv-{}@{}.service",
        systemd::escape_unit_part(slug),
        systemd::unique_suffix()
    );
    mgr.run_transient(
        &unit,
        &argv,
        description,
        vec![("ExitType", "cgroup".into())],
    )
    .await?;
    Ok(unit)
}
pub fn dbus_object_path(app_id: &str) -> String {
    format!("/{}", app_id.replace('.', "/").replace('-', "_"))
}
async fn activate(env: &Env, app: &Application) -> ActionResult {
    let app_id = app.desktop_id.trim_end_matches(".desktop");
    if app.dbus_activatable {
        let conn = zbus::Connection::session().await?;
        let platform: std::collections::HashMap<&str, zvariant::Value<'_>> = Default::default();
        let r = conn
            .call_method(
                Some(app_id),
                dbus_object_path(app_id).as_str(),
                Some("org.freedesktop.Application"),
                "Activate",
                &(platform,),
            )
            .await;
        if r.is_ok() {
            return Ok(String::new());
        }
    }
    let mut argv = exec_argv(&app.raw_exec, Some(app));
    if app.terminal {
        argv.splice(0..0, ["alacritty".to_string(), "-e".to_string()]);
    }
    start_unit(env, app_id, argv, &app.name).await
}

pub fn spawn(ctx: &Ctx) -> ApplicationsHandle {
    let topic = crate::topic::<ApplicationsTopic>(ctx, ApplicationState::default());
    let (handle, mut rx) = Handle::<ApplicationState, AppAction>::new(topic.clone());
    let env = ctx.env.clone();
    let mut shutdown = ctx.shutdown.clone();
    ctx.rt.spawn(async move {
        let dirs = search_dirs(&env);
        loop {
            let (apps, watched) = {
                let d = dirs.clone();
                tokio::task::spawn_blocking(move || index(&d)).await.unwrap_or_default()
            };
            // Missing roots are watched through their nearest existing ancestor.
            let specs: Vec<(PathBuf, &[&str])> =
                dirs.iter().chain(watched.iter()).map(|d| (d.clone(), &[][..])).collect();
            let watch = util::Watch::new(&specs).ok();
            topic.publish(ApplicationState { applications: apps });
            let changed = async {
                match &watch {
                    Some(w) => {
                        let _ = util::settle(w, Duration::from_millis(250)).await;
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = changed => {}
                msg = rx.recv() => {
                    let Some(envl) = msg else { return };
                    let apps = topic.snapshot().value.applications;
                    let action = envl.action.clone();
                    let env2 = env.clone();
                    // Detached: activation outlives the initiating popup.
                    tokio::spawn(async move {
                        let r = match action {
                            AppAction::Activate(id) => match apps.iter().find(|a| a.desktop_id == id) {
                                Some(a) => activate(&env2, a).await,
                                None => Err(ServiceError::new("not_found", format!("unknown application {id}"))),
                            },
                            AppAction::ActivateByName(n) => match apps.iter().find(|a| a.name == n) {
                                Some(a) => activate(&env2, a).await,
                                None => Err(ServiceError::new("not_found", format!("unknown application {n}"))),
                            },
                            AppAction::Launch(t) => match launch_argv(&env2, &t) {
                                Ok(argv) => {
                                    let slug = util::basename(&argv[0]).to_string();
                                    start_unit(&env2, &slug, argv, "HyprV launch").await
                                }
                                Err(e) => Err(e),
                            },
                            AppAction::OpenUri(uri) => open_uri(&uri).await,
                        };
                        envl.respond(r);
                    });
                }
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
        }
    });
    handle
}
async fn open_uri(uri: &str) -> ActionResult {
    let conn = zbus::Connection::session().await?;
    let options: std::collections::HashMap<&str, zvariant::Value<'_>> = Default::default();
    conn.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.OpenURI"),
        "OpenURI",
        &("", uri, options),
    )
    .await?;
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fetcher_rules() {
        let text = "[Desktop Entry]\nName=Foo\nName[de]=Fu\nExec=foo --x %U @@u %u @@\nIcon=foo\n[Desktop Action new]\nName=Bar\nExec=bar\n";
        let a = parse_entry(text, Path::new("/a/foo.desktop"), "foo.desktop".into()).unwrap();
        assert_eq!(
            (a.name.as_str(), a.exec.as_str(), a.icon.as_str()),
            ("Foo", "foo --x", "foo")
        );
        assert!(parse_entry(
            "[Desktop Entry]\nName=X\nExec=x\nNoDisplay=true\n",
            Path::new("/x"),
            "x".into()
        )
        .is_none());
    }
    #[test]
    fn exec_field_codes() {
        let a = parse_entry(
            "[Desktop Entry]\nName=Ed\nExec=x\nIcon=ed\n",
            Path::new("/p/ed.desktop"),
            "ed.desktop".into(),
        )
        .unwrap();
        assert_eq!(
            exec_argv("ed %F --name %c %i \"a b\" 100%%", Some(&a)),
            ["ed", "--name", "Ed", "--icon", "ed", "a b", "100%"]
        );
        assert_eq!(
            exec_argv(r#"sh -c "echo \"hi\"""#, None),
            ["sh", "-c", "echo \"hi\""]
        );
        assert_eq!(dbus_object_path("org.gnome.Foo-Bar"), "/org/gnome/Foo_Bar");
    }
    #[test]
    fn index_dedupes_later_wins_and_sorts() {
        let root = std::env::temp_dir().join(format!("hyprv-apps-{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b/sub"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            a.join("z.desktop"),
            "[Desktop Entry]\nName=zeta\nExec=one\n",
        )
        .unwrap();
        std::fs::write(a.join("y.desktop"), "[Desktop Entry]\nName=Alpha\nExec=a\n").unwrap();
        std::fs::write(
            b.join("z2.desktop"),
            "[Desktop Entry]\nName=zeta\nExec=two\n",
        )
        .unwrap();
        let (apps, watched) = index(&[a.clone(), root.join("b")]);
        let names: Vec<_> = apps
            .iter()
            .map(|a| (a.name.as_str(), a.exec.as_str()))
            .collect();
        assert_eq!(names, [("Alpha", "a"), ("zeta", "two")]);
        assert_eq!(apps[1].desktop_id, "sub-z2.desktop");
        assert_eq!(watched.len(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }
}
