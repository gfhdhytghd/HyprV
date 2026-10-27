//! Native port of `apply-ui-state.sh apply_links_and_theme`. Each application is
//! independent; failures are collected per app and never retried periodically.
use super::{files, legacy, Theme, Variant};
use crate::{agent_island::focus as focus_kitty, hypr::Hypr, util, Env, ServiceError};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    time::Duration,
};

pub type Errors = BTreeMap<String, ServiceError>;

pub struct Plan {
    pub gtk_theme: String,
    pub icon_theme: String,
    pub kvantum: String,
    pub cursor: &'static str,
    pub cursor_size: &'static str,
    pub prefer_dark: &'static str,
    pub qt6_scheme: &'static str,
    pub qt5_scheme: &'static str,
    pub kde_scheme: &'static str,
    pub kde_laf: &'static str,
    pub color_scheme: &'static str,
    pub mode_label: &'static str,
    pub suffix: &'static str,
}
pub fn plan(theme: Theme) -> Plan {
    let dark = theme == Theme::Dark;
    let mode_label = theme.label();
    Plan {
        gtk_theme: format!("Orchis-{mode_label}-Compact"),
        icon_theme: format!("Fluent{}", if dark { "-dark" } else { "-light" }),
        kvantum: format!("Orchis{}", if dark { "Dark" } else { "" }),
        cursor: "Adwaita",
        cursor_size: "24",
        prefer_dark: if dark { "true" } else { "false" },
        qt6_scheme: if dark {
            "/usr/share/qt6ct/colors/darker.conf"
        } else {
            "/usr/share/qt6ct/colors/simple.conf"
        },
        qt5_scheme: if dark {
            "/usr/share/qt5ct/colors/darker.conf"
        } else {
            "/usr/share/qt5ct/colors/simple.conf"
        },
        kde_scheme: if dark { "OrchisDark" } else { "Orchis" },
        kde_laf: if dark {
            "com.github.vinceliuice.Orchis-dark"
        } else {
            "com.github.vinceliuice.Orchis"
        },
        color_scheme: if dark { "prefer-dark" } else { "prefer-light" },
        mode_label,
        suffix: if dark { "-dark" } else { "" },
    }
}

fn record(errors: &mut Errors, app: &str, r: Result<(), ServiceError>) {
    if let Err(e) = r {
        errors.insert(app.into(), e);
    }
}
fn io(r: std::io::Result<()>) -> Result<(), ServiceError> {
    r.map_err(ServiceError::from)
}

pub async fn apply(env: &Env, hypr: &Hypr, theme: Theme, variant: Variant) -> Errors {
    let mut errors = Errors::new();
    let p = plan(theme);
    let h = &env.home;
    let cfg = env.hyprv_config_dir.clone();
    let sfx = p.suffix;
    let v = variant.as_str();
    // Toolkit color scheme first: Chrome/Electron follow it through the settings portal,
    // so it must not wait behind terminal, wallpaper or KDE reloads.
    // XFConf xsettings channel (xfconf-query equivalents).
    let xf = [
        ("/Net/ThemeName", format!("Adwaita{sfx}")),
        ("/Net/IconThemeName", format!("Adwaita{sfx}")),
        ("/Gtk/CursorThemeName", p.cursor.to_string()),
    ];
    for (prop, value) in xf {
        record(
            &mut errors,
            "xfconf",
            xfconf(prop, zvariant::Value::from(value)).await,
        );
    }
    record(
        &mut errors,
        "xfconf",
        xfconf("/Gtk/CursorThemeSize", zvariant::Value::I32(24)).await,
    );
    // GSettings through the dconf writer service.
    record(
        &mut errors,
        "gsettings",
        super::dconf::write(&[
            (
                "/org/gnome/desktop/interface/gtk-theme",
                zvariant::Value::from(p.gtk_theme.as_str()),
            ),
            (
                "/org/gnome/desktop/interface/icon-theme",
                zvariant::Value::from(p.icon_theme.as_str()),
            ),
            (
                "/org/gnome/desktop/interface/cursor-theme",
                zvariant::Value::from(p.cursor),
            ),
            (
                "/org/gnome/desktop/interface/cursor-size",
                zvariant::Value::I32(24),
            ),
            (
                "/org/gnome/desktop/interface/color-scheme",
                zvariant::Value::from(p.color_scheme),
            ),
        ])
        .await,
    );
    let links: [(&str, PathBuf, PathBuf); 5] = [
        (
            "wofi",
            cfg.join(format!("wofi/style/{v}-style{sfx}.css")),
            h.join(".config/wofi/style.css"),
        ),
        (
            "rofi",
            cfg.join(format!("rofi/colors{sfx}.rasi")),
            h.join(".config/rofi/colors.rasi"),
        ),
        (
            "alacritty",
            cfg.join(format!("alacritty/alacritty{sfx}.toml")),
            h.join(".config/alacritty/alacritty.toml"),
        ),
        (
            "ghostty",
            cfg.join(format!("ghostty/ghostty{sfx}.toml")),
            h.join(".config/ghostty/config"),
        ),
        (
            "kitty",
            cfg.join("kitty/kitty.conf"),
            h.join(".config/kitty/kitty.conf"),
        ),
    ];
    for (app, target, link) in &links {
        if let Some(parent) = link.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if target.is_file() {
            record(&mut errors, app, io(util::atomic_symlink(target, link)));
        }
    }
    let _ = std::fs::create_dir_all(h.join(".config/swaync"));
    let kitty_theme = cfg.join(format!("kitty/theme{sfx}.conf"));
    if kitty_theme.is_file() {
        record(
            &mut errors,
            "kitty",
            io(util::atomic_symlink(
                &kitty_theme,
                &h.join(".config/kitty/current-theme.conf"),
            )),
        );
        record(&mut errors, "kitty", reload_kitty(env, &kitty_theme).await);
    }
    let swaync = cfg.join(format!("swaync/style{sfx}.css"));
    if swaync.is_file() {
        record(
            &mut errors,
            "swaync",
            io(util::atomic_symlink(
                &swaync,
                &h.join(".config/swaync/style.css"),
            )),
        );
        // `swaync-client -rs` (ReloadCss), bounded like `timeout 2`.
        let r = tokio::time::timeout(
            Duration::from_secs(2),
            session_call(
                "org.erikreider.swaync.cc",
                "/org/erikreider/swaync/cc",
                "org.erikreider.swaync.cc",
                "ReloadCss",
                &(),
            ),
        )
        .await;
        if let Ok(Err(e)) = r {
            if !is_absent(&e) {
                errors.insert("swaync".into(), e);
            }
        }
    }
    record(&mut errors, "ghostty", reload_ghostty(hypr).await);

    record(
        &mut errors,
        "activation-environment",
        activation_env(env, &p).await,
    );
    record(
        &mut errors,
        "hyprland-cursor",
        hypr.plain(&format!("setcursor {} {}", p.cursor, p.cursor_size))
            .await
            .map(|_| ()),
    );

    let gtk3 = h.join(".config/gtk-3.0/settings.ini");
    let gtk4 = h.join(".config/gtk-4.0/settings.ini");
    for path in [&gtk3, &gtk4] {
        let r = files::edit(path, |t| {
            let t = files::set_ini_value(t, "Settings", "gtk-theme-name", &p.gtk_theme);
            let t = files::set_ini_value(&t, "Settings", "gtk-icon-theme-name", &p.icon_theme);
            let t = files::set_ini_value(&t, "Settings", "gtk-cursor-theme-name", p.cursor);
            let t = files::set_ini_value(&t, "Settings", "gtk-cursor-theme-size", p.cursor_size);
            files::set_ini_value(
                &t,
                "Settings",
                "gtk-application-prefer-dark-theme",
                p.prefer_dark,
            )
        });
        record(&mut errors, "gtk", io(r));
    }
    for path in [h.join(".gtkrc-2.0"), h.join(".config/gtkrc-2.0")] {
        let r = files::edit(&path, |t| {
            let t = files::set_assignment(t, "gtk-theme-name", &format!("\"{}\"", p.gtk_theme));
            let t =
                files::set_assignment(&t, "gtk-icon-theme-name", &format!("\"{}\"", p.icon_theme));
            let t =
                files::set_assignment(&t, "gtk-cursor-theme-name", &format!("\"{}\"", p.cursor));
            files::set_assignment(&t, "gtk-cursor-theme-size", p.cursor_size)
        });
        record(&mut errors, "gtk2", io(r));
    }
    let xs = h.join(".config/xsettingsd/xsettingsd.conf");
    let r = files::edit(&xs, |t| {
        let t = files::set_xsettings_value(t, "Net/ThemeName", &format!("\"{}\"", p.gtk_theme));
        let t =
            files::set_xsettings_value(&t, "Net/IconThemeName", &format!("\"{}\"", p.icon_theme));
        let t = files::set_xsettings_value(&t, "Net/CursorThemeName", &format!("\"{}\"", p.cursor));
        files::set_xsettings_value(&t, "Net/CursorThemeSize", p.cursor_size)
    });
    record(&mut errors, "xsettingsd", io(r));
    record(&mut errors, "xsettingsd", legacy::restart_xsettingsd(env));

    for (path, scheme) in [
        (h.join(".config/qt6ct/qt6ct.conf"), p.qt6_scheme),
        (h.join(".config/qt5ct/qt5ct.conf"), p.qt5_scheme),
    ] {
        let r = files::edit(&path, |t| {
            let t = files::set_ini_value(t, "Appearance", "style", "kvantum");
            let t = files::set_ini_value(&t, "Appearance", "icon_theme", &p.icon_theme);
            files::set_ini_value(&t, "Appearance", "color_scheme_path", scheme)
        });
        record(&mut errors, "qtct", io(r));
    }
    record(
        &mut errors,
        "kvantum",
        io(files::edit(
            &h.join(".config/Kvantum/kvantum.kvconfig"),
            |t| files::set_ini_value(t, "General", "theme", &p.kvantum),
        )),
    );
    record(
        &mut errors,
        "plasma-colorscheme",
        legacy::plasma_colorscheme(env, p.kde_scheme).await,
    );
    if util::which("kwriteconfig6", env.var("PATH")).is_some() {
        let r = files::edit(&h.join(".config/kdeglobals"), |t| {
            let t = files::set_ini_value(t, "KDE", "LookAndFeelPackage", p.kde_laf);
            let t = files::set_ini_value(
                &t,
                "KDE",
                "DefaultLightLookAndFeel",
                "com.github.vinceliuice.Orchis",
            );
            let t = files::set_ini_value(
                &t,
                "KDE",
                "DefaultDarkLookAndFeel",
                "com.github.vinceliuice.Orchis-dark",
            );
            let t = files::set_ini_value(&t, "Icons", "Theme", &p.icon_theme);
            files::set_ini_value(&t, "KDE", "widgetStyle", "qt6ct-style")
        });
        record(&mut errors, "kdeglobals", io(r));
    }
    let background = cfg.join(format!("backgrounds/{v}-background{sfx}.jpg"));
    record(
        &mut errors,
        "wallpaper",
        legacy::wallpaper(env, &background).await,
    );
    if util::which("kwriteconfig6", env.var("PATH")).is_some() {
        record(
            &mut errors,
            "konsole",
            io(files::edit(&h.join(".config/konsolerc"), |t| {
                files::set_ini_value(
                    t,
                    "Desktop Entry",
                    "DefaultProfile",
                    &format!("{}.profile", p.mode_label),
                )
            })),
        );
    }
    record(&mut errors, "konsole", konsole_profiles(p.mode_label).await);
    record(
        &mut errors,
        "wechat",
        legacy::sync_wechat(env, hypr, theme == Theme::Dark).await,
    );
    errors
}

fn is_absent(e: &ServiceError) -> bool {
    e.message.contains("ServiceUnknown")
        || e.message.contains("NameHasNoOwner")
        || e.message.contains("was not provided")
}
async fn session_call<B>(
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> Result<zbus::Message, ServiceError>
where
    B: serde::Serialize + zvariant::DynamicType,
{
    let conn = zbus::Connection::session().await?;
    Ok(conn
        .call_method(Some(dest), path, Some(iface), method, body)
        .await?)
}
async fn xfconf(prop: &str, value: zvariant::Value<'_>) -> Result<(), ServiceError> {
    match session_call(
        "org.xfce.Xfconf",
        "/org/xfce/Xfconf",
        "org.xfce.Xfconf",
        "SetProperty",
        &("xsettings", prop, value),
    )
    .await
    {
        Ok(_) => Ok(()),
        // xfconf-query absent: the source skips silently.
        Err(e) if is_absent(&e) => Ok(()),
        Err(e) => Err(e),
    }
}
async fn activation_env(env: &Env, p: &Plan) -> Result<(), ServiceError> {
    let desktop = env
        .var("XDG_CURRENT_DESKTOP")
        .unwrap_or("Hyprland")
        .to_string();
    let session = env
        .var("XDG_SESSION_DESKTOP")
        .unwrap_or("Hyprland")
        .to_string();
    let vars: HashMap<&str, String> = HashMap::from([
        ("GTK_THEME", p.gtk_theme.clone()),
        ("QT_QPA_PLATFORMTHEME", "qt6ct".into()),
        ("XCURSOR_THEME", p.cursor.into()),
        ("XCURSOR_SIZE", p.cursor_size.into()),
        ("XDG_CURRENT_DESKTOP", desktop),
        ("XDG_SESSION_DESKTOP", session),
    ]);
    session_call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "UpdateActivationEnvironment",
        &(vars.clone(),),
    )
    .await?;
    let assignments: Vec<String> = vars.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let refs: Vec<&str> = assignments.iter().map(String::as_str).collect();
    let mgr = crate::systemd::UserManager::connect().await?;
    mgr.manager.set_environment(&refs).await?;
    Ok(())
}
async fn reload_ghostty(hypr: &Hypr) -> Result<(), ServiceError> {
    let clients: Vec<Value> = match hypr.json("clients").await {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    for c in clients {
        let class = format!(
            "{} {}",
            c["class"].as_str().unwrap_or(""),
            c["initialClass"].as_str().unwrap_or("")
        )
        .to_lowercase();
        let Some(addr) = c["address"].as_str() else {
            continue;
        };
        if !class.contains("ghostty") || !crate::hypr::valid_address(addr) {
            continue;
        }
        let lua = format!("hl.dispatch(hl.dsp.send_shortcut({{ mods = 'CTRL SHIFT', key = 'comma', window = 'address:{addr}' }}))");
        if hypr.eval(&lua).await.is_err() {
            let _ = hypr
                .dispatch(&format!("sendshortcut CTRL SHIFT, comma, address:{addr}"))
                .await;
        }
    }
    Ok(())
}

/// Colors accepted by kitty `set-colors` from a theme file.
pub fn kitty_colors(text: &str) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
            continue;
        };
        let value = value.trim();
        let colorish = key == "foreground"
            || key == "background"
            || key.ends_with("_color")
            || key.ends_with("_foreground")
            || key.ends_with("_background")
            || key == "cursor"
            || key
                .strip_prefix("color")
                .is_some_and(|n| n.parse::<u8>().is_ok());
        if !colorish {
            continue;
        }
        if value == "none" {
            out.insert(key.into(), Value::Null);
        } else if let Some(hex) = value.strip_prefix('#').filter(|h| h.len() == 6) {
            if let Ok(n) = u32::from_str_radix(hex, 16) {
                out.insert(key.into(), json!(n));
            }
        }
    }
    out
}
async fn reload_kitty(env: &Env, theme_file: &Path) -> Result<(), ServiceError> {
    if util::which("kitty", env.var("PATH")).is_none() {
        return Ok(());
    }
    let colors = kitty_colors(&std::fs::read_to_string(theme_file)?);
    let tmp = env.var("TEMP").unwrap_or("/tmp").to_string();
    let mut sockets = Vec::new();
    for dir in [env.runtime_dir.clone(), PathBuf::from(tmp)] {
        if let Ok(it) = std::fs::read_dir(&dir) {
            for e in it.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with("kitty-") && n.ends_with(".sock") {
                    sockets.push(e.path());
                }
            }
        }
    }
    for sock in sockets {
        let addr = format!("unix:{}", sock.display());
        let payload = json!({"colors": colors, "match_window": null, "match_tab": null,
            "all": true, "configured": true, "reset": false});
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            focus_kitty::kitty_command(&addr, "set-colors", payload),
        )
        .await;
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            focus_kitty::kitty_command(
                &addr,
                "load-config",
                json!({"paths": [], "override": [], "ignore_overrides": false}),
            ),
        )
        .await;
    }
    // `pkill -USR1 -x kitty`
    let proc = util::Proc::default();
    for pid in proc.pids() {
        if proc.stat(pid).is_some_and(|s| s.comm == "kitty") {
            if let Ok(fd) = util::Pidfd::open(pid) {
                let _ = fd.signal(libc::SIGUSR1);
            }
        }
    }
    Ok(())
}
fn child_nodes(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<node name=\"") {
        rest = &rest[i + 12..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
        }
    }
    out
}
async fn konsole_profiles(profile: &str) -> Result<(), ServiceError> {
    let conn = zbus::Connection::session().await?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
    for name in dbus.list_names().await? {
        let service = name.as_str();
        if !service.contains("org.kde.konsole") {
            continue;
        }
        for (base, iface, method) in [
            ("/Sessions", "org.kde.konsole.Session", "setProfile"),
            ("/Windows", "org.kde.konsole.Window", "setDefaultProfile"),
        ] {
            let Ok(reply) = conn
                .call_method(
                    Some(service),
                    base,
                    Some("org.freedesktop.DBus.Introspectable"),
                    "Introspect",
                    &(),
                )
                .await
            else {
                continue;
            };
            let xml: String = reply.body().deserialize().unwrap_or_default();
            for child in child_nodes(&xml) {
                let path = format!("{base}/{child}");
                let _ = conn
                    .call_method(
                        Some(service),
                        path.as_str(),
                        Some(iface),
                        method,
                        &(profile,),
                    )
                    .await;
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_names() {
        let d = plan(Theme::Dark);
        assert_eq!(d.gtk_theme, "Orchis-Dark-Compact");
        assert_eq!(d.icon_theme, "Fluent-dark");
        assert_eq!(d.kvantum, "OrchisDark");
        let l = plan(Theme::Light);
        assert_eq!(l.gtk_theme, "Orchis-Light-Compact");
        assert_eq!(l.icon_theme, "Fluent-light");
        assert_eq!(l.kvantum, "Orchis");
        assert_eq!(l.color_scheme, "prefer-light");
    }
    #[test]
    fn kitty_theme_parse() {
        let c = kitty_colors("# x\nforeground #fcfcfc\ncolor1 #ff0000\nwayland_titlebar_color system\nfont_size 11\nmark1_background none\n");
        assert_eq!(c["foreground"], 0xfcfcfc);
        assert_eq!(c["color1"], 0xff0000);
        assert!(c["mark1_background"].is_null());
        assert!(!c.contains_key("wayland_titlebar_color") && !c.contains_key("font_size"));
    }
    #[test]
    fn introspection_children() {
        assert_eq!(
            child_nodes("<node><node name=\"1\"/><node name=\"2\"/></node>"),
            ["1", "2"]
        );
    }
}
