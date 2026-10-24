//! RAM stick RGB follows the shell theme: OpenRGB devices 0 and 1 (`ENE DRAM`) are
//! switched to `Off` in dark mode and restored to their captured mode in light mode.
//! The motherboard (device 2) and anything not named `ENE DRAM` is never addressed.
//!
//! No OpenRGB SDK server runs on this desktop, so each sync is one-shot `openrgb
//! --noautoconnect` invocations (a list, then at most one set). The worker is a
//! single task fed by a watch channel: it debounces rapid toggles, never runs two
//! syncs at once (an `flock` also serializes against other shell processes), and
//! only logs failures. It syncs once at start because the BIOS resets RGB at boot.
//!
//! The mode in effect before `Off` is persisted in `$XDG_STATE_HOME/hyprv/ram-rgb.env`.
//! The CLI cannot read colors, so only the mode is restored and the device keeps the
//! colors it already holds; none are ever written. A device that is not `Off` in
//! light mode was changed by the user and is left alone.
use super::Theme;
use crate::{util, Env};
use std::{
    collections::BTreeMap,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::watch;

pub const DEVICES: [usize; 2] = [0, 1];
pub const DEVICE_NAME: &str = "ENE DRAM";
pub const OFF: &str = "Off";

#[derive(Clone, Debug)]
pub struct Config {
    pub bin: PathBuf,
    pub saved: PathBuf,
    pub lock: PathBuf,
    pub debounce: Duration,
    pub timeout: Duration,
}
impl Config {
    /// `None` unless this is the user's own theme state: harnesses and nested test
    /// shells relocate `XDG_STATE_HOME` or set `HYPRV_NESTED_*`, and `HYPRV_RAM_RGB=off`
    /// disables it explicitly. `HYPRV_OPENRGB` overrides the binary.
    pub fn from_env(env: &Env) -> Option<Self> {
        let disabled = env.var("HYPRV_RAM_RGB") == Some("off")
            || env.var("HYPRV_NESTED_TEST").is_some()
            || env.var("HYPRV_NESTED_FIXTURE").is_some()
            || env.state_home != env.home.join(".local/state");
        (!disabled).then(|| Self {
            bin: env.var("HYPRV_OPENRGB").unwrap_or("openrgb").into(),
            saved: env.state_home.join("hyprv/ram-rgb.env"),
            lock: env.runtime_dir.join("hyprv-ram-rgb.lock"),
            debounce: Duration::from_millis(750),
            timeout: Duration::from_secs(30),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Device {
    pub index: usize,
    pub name: String,
    pub modes: Vec<String>,
    pub current: Option<String>,
}

/// `openrgb -ld`: `N: NAME` headers, `  Modes: A [Current] 'Two Words' ...`.
pub fn parse_devices(text: &str) -> Vec<Device> {
    let mut out: Vec<Device> = Vec::new();
    for line in text.lines() {
        if let Some((n, name)) = line.split_once(": ") {
            if let Ok(index) = n.parse() {
                out.push(Device {
                    index,
                    name: name.trim().to_string(),
                    ..Default::default()
                });
                continue;
            }
        }
        let (Some(dev), Some(modes)) = (out.last_mut(), line.trim().strip_prefix("Modes:")) else {
            continue;
        };
        for token in mode_tokens(modes) {
            let (name, current) = match token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
                Some(t) => (t, true),
                None => (token.as_str(), false),
            };
            let name = name.trim_matches('\'').to_string();
            if current {
                dev.current = Some(name.clone());
            }
            dev.modes.push(name);
        }
    }
    out
}
fn mode_tokens(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    for c in s.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                cur.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn load_saved(path: &Path) -> BTreeMap<usize, String> {
    util::read_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().parse().ok()?, v.trim().to_string()))
        })
        .filter(|(_, v): &(usize, String)| !v.is_empty() && v != OFF)
        .collect()
}
fn store_saved(path: &Path, saved: &BTreeMap<usize, String>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::from("# RAM RGB modes captured before dark mode turned them off\n");
    for (i, m) in saved {
        text.push_str(&format!("{i}={m}\n"));
    }
    util::atomic_write(path, text.as_bytes(), None)
}

async fn openrgb(cfg: &Config, args: &[String]) -> Result<String, String> {
    let child = tokio::process::Command::new(&cfg.bin)
        .arg("--noautoconnect")
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(cfg.timeout, child)
        .await
        .map_err(|_| format!("{} {args:?} timed out", cfg.bin.display()))?
        .map_err(|e| format!("{}: {e}", cfg.bin.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("openrgb {args:?}: {} {}", out.status, err.trim()));
    }
    Ok(stdout)
}

/// Exclusive lock across shell processes, held for one sync.
async fn lock(path: &Path) -> Result<std::fs::File, String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(format!("flock: {}", std::io::Error::last_os_error()));
        }
        Ok(f)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One sync: returns the `-d N -m MODE` pairs that were applied.
pub async fn sync(cfg: &Config, theme: Theme) -> Result<Vec<(usize, String)>, String> {
    let _lock = lock(&cfg.lock).await?;
    let devices = parse_devices(&openrgb(cfg, &["-ld".into()]).await?);
    let mut targets = Vec::new();
    for i in DEVICES {
        let d = devices
            .iter()
            .find(|d| d.index == i)
            .filter(|d| d.name == DEVICE_NAME && d.current.is_some())
            .ok_or_else(|| {
                format!("device {i} is not a detected {DEVICE_NAME}; nothing changed")
            })?;
        targets.push(d);
    }
    let mut saved = load_saved(&cfg.saved);
    let mut apply = Vec::new();
    match theme {
        Theme::Dark => {
            for d in &targets {
                let cur = d.current.clone().unwrap_or_default();
                if cur != OFF {
                    saved.insert(d.index, cur);
                    apply.push((d.index, OFF.to_string()));
                }
            }
            if !apply.is_empty() {
                // Persist before switching off so a failure never loses the mode.
                store_saved(&cfg.saved, &saved)
                    .map_err(|e| format!("{}: {e}", cfg.saved.display()))?;
            }
        }
        Theme::Light => {
            for d in targets.iter().filter(|d| d.current.as_deref() == Some(OFF)) {
                match saved.get(&d.index) {
                    Some(m) if d.modes.contains(m) => apply.push((d.index, m.clone())),
                    Some(m) => eprintln!(
                        "hyprv ram-rgb: saved mode {m:?} unsupported by device {}; left off",
                        d.index
                    ),
                    None => eprintln!(
                        "hyprv ram-rgb: no saved mode for device {}; left off",
                        d.index
                    ),
                }
            }
        }
    }
    if !apply.is_empty() {
        let args: Vec<String> = apply
            .iter()
            .flat_map(|(i, m)| ["-d".into(), i.to_string(), "-m".into(), m.clone()])
            .collect();
        openrgb(cfg, &args).await?;
    }
    Ok(apply)
}

/// Spawns the worker; send the current theme whenever it may have changed.
pub fn spawn(
    rt: &tokio::runtime::Handle,
    cfg: Config,
    mut shutdown: watch::Receiver<bool>,
) -> watch::Sender<Option<Theme>> {
    let (tx, mut rx) = watch::channel(None::<Theme>);
    rt.spawn(async move {
        loop {
            tokio::select! {
                r = rx.changed() => if r.is_err() { return },
                _ = shutdown.changed() => if *shutdown.borrow() { return },
            }
            // Debounce: wait until the theme has been quiet for `debounce`.
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(cfg.debounce) => break,
                    r = rx.changed() => if r.is_err() { return },
                    _ = shutdown.changed() => if *shutdown.borrow() { return },
                }
            }
            let Some(theme) = *rx.borrow_and_update() else {
                continue;
            };
            match sync(&cfg, theme).await {
                Ok(applied) if !applied.is_empty() => {
                    eprintln!("hyprv ram-rgb: {} -> {applied:?}", theme.as_str())
                }
                Ok(_) => {}
                Err(e) => eprintln!("hyprv ram-rgb: {} failed: {e}", theme.as_str()),
            }
        }
    });
    tx
}
/// Publish `theme` to the worker only when it differs from the last value.
pub fn request(tx: &watch::Sender<Option<Theme>>, theme: Theme) {
    tx.send_if_modified(|v| {
        let changed = *v != Some(theme);
        *v = Some(theme);
        changed
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake `openrgb` keeping per-device modes in files; it records every call and
    /// fails if two invocations overlap.
    const FAKE: &str = r#"#!/bin/sh
d="$FAKE_DIR"
mkdir "$d/running" 2>/dev/null || { echo overlap >> "$d/calls"; exit 3; }
trap 'rmdir "$d/running"' EXIT
echo "$*" >> "$d/calls"
[ "$1" = --noautoconnect ] || exit 4
shift
sleep "$(cat "$d/sleep" 2>/dev/null || echo 0)"
[ -f "$d/fail" ] && { echo boom >&2; exit 1; }
if [ "$1" = -ld ]; then
  for i in 0 1 2; do
    cur=$(cat "$d/mode$i")
    name="ENE DRAM"; [ $i = 2 ] && name="ASUS ROG STRIX B860-I GAMING WIFI"
    [ -f "$d/name$i" ] && name=$(cat "$d/name$i")
    printf '%s: %s\n  Type:           DRAM\n  Modes:' "$i" "$name"
    for m in Direct Off Static "Spectrum Cycle" Rainbow; do
      q="$m"; case "$m" in *" "*) q="'$m'";; esac
      if [ "$m" = "$cur" ]; then printf ' [%s]' "$q"; else printf ' %s' "$q"; fi
    done
    printf '\n  Zones: DRAM\n\n'
  done
  exit 0
fi
while [ $# -gt 0 ]; do
  [ "$1" = -d ] && [ "$3" = -m ] || exit 5
  printf '%s' "$4" > "$d/mode$2"
  shift 4
done
"#;
    struct Fake {
        dir: PathBuf,
        cfg: Config,
    }
    impl Fake {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("hyprv-ram-rgb-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let bin = dir.join("openrgb");
            std::fs::write(&bin, FAKE.replace("$FAKE_DIR", &dir.display().to_string())).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            for (i, m) in ["Rainbow", "Static", "Direct"].iter().enumerate() {
                std::fs::write(dir.join(format!("mode{i}")), m).unwrap();
            }
            let cfg = Config {
                bin,
                saved: dir.join("state/hyprv/ram-rgb.env"),
                lock: dir.join("lock"),
                debounce: Duration::from_millis(150),
                timeout: Duration::from_secs(10),
            };
            Self { dir, cfg }
        }
        fn mode(&self, i: usize) -> String {
            std::fs::read_to_string(self.dir.join(format!("mode{i}"))).unwrap()
        }
        fn set(&self, i: usize, m: &str) {
            std::fs::write(self.dir.join(format!("mode{i}")), m).unwrap();
        }
        fn calls(&self) -> Vec<String> {
            util::read_string(&self.dir.join("calls"))
                .unwrap_or_default()
                .lines()
                .map(String::from)
                .collect()
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn parses_real_list_output() {
        let text = "0: ENE DRAM\n  Type:           DRAM\n  Modes: Direct Off Static Breathing Flashing 'Spectrum Cycle' [Rainbow] 'Chase Fade' Chase 'Random Flicker'\n  Zones: DRAM\n\n2: ASUS ROG STRIX B860-I GAMING WIFI\n  Modes: [Direct] Off ['Spectrum Cycle Wave']\n";
        let d = parse_devices(text);
        assert_eq!(d.len(), 2);
        assert_eq!((d[0].index, d[0].name.as_str()), (0, "ENE DRAM"));
        assert_eq!(d[0].current.as_deref(), Some("Rainbow"));
        assert_eq!(d[0].modes.len(), 10);
        assert!(d[0].modes.iter().any(|m| m == "Random Flicker"));
        assert_eq!(d[1].index, 2);
        assert_eq!(d[1].current.as_deref(), Some("Spectrum Cycle Wave"));
    }

    #[tokio::test]
    async fn dark_captures_and_turns_off_light_restores_motherboard_untouched() {
        let f = Fake::new("cycle");
        f.set(1, "Spectrum Cycle");
        let applied = sync(&f.cfg, Theme::Dark).await.unwrap();
        assert_eq!(applied, vec![(0, OFF.into()), (1, OFF.into())]);
        assert_eq!(
            (f.mode(0), f.mode(1), f.mode(2)),
            ("Off".into(), "Off".into(), "Direct".into())
        );
        let saved = load_saved(&f.cfg.saved);
        assert_eq!(saved.get(&0).map(String::as_str), Some("Rainbow"));
        assert_eq!(saved.get(&1).map(String::as_str), Some("Spectrum Cycle"));

        // Already off (e.g. shell restart in dark mode): nothing applied, save kept.
        assert!(sync(&f.cfg, Theme::Dark).await.unwrap().is_empty());
        assert_eq!(load_saved(&f.cfg.saved), saved);

        let applied = sync(&f.cfg, Theme::Light).await.unwrap();
        assert_eq!(
            applied,
            vec![(0, "Rainbow".into()), (1, "Spectrum Cycle".into())]
        );
        assert_eq!(
            (f.mode(0), f.mode(1), f.mode(2)),
            ("Rainbow".into(), "Spectrum Cycle".into(), "Direct".into())
        );
        let calls = f.calls();
        assert!(
            calls.iter().all(|c| c.starts_with("--noautoconnect ")),
            "{calls:?}"
        );
        assert!(!calls.iter().any(|c| c.contains("-d 2")), "{calls:?}");
        assert!(
            calls.iter().all(|c| !c.contains(" -c ")),
            "colors must never be set: {calls:?}"
        );
    }

    #[tokio::test]
    async fn light_leaves_user_changes_and_unknown_modes_alone() {
        let f = Fake::new("light");
        // Light at boot: LEDs on, nothing saved -> only a list.
        assert!(sync(&f.cfg, Theme::Light).await.unwrap().is_empty());
        assert_eq!(f.calls().len(), 1);
        // Off without a saved mode stays off rather than inventing one.
        f.set(0, OFF);
        assert!(sync(&f.cfg, Theme::Light).await.unwrap().is_empty());
        assert_eq!(f.mode(0), OFF);
        // The user picked Static meanwhile on device 1: not overwritten.
        store_saved(
            &f.cfg.saved,
            &BTreeMap::from([(0, "Rainbow".into()), (1, "Rainbow".into())]),
        )
        .unwrap();
        f.set(1, "Static");
        assert_eq!(
            sync(&f.cfg, Theme::Light).await.unwrap(),
            vec![(0, "Rainbow".into())]
        );
        assert_eq!(f.mode(1), "Static");
    }

    #[tokio::test]
    async fn wrong_device_or_failure_changes_nothing() {
        let f = Fake::new("guard");
        std::fs::write(f.dir.join("name1"), "Some Keyboard").unwrap();
        let e = sync(&f.cfg, Theme::Dark).await.unwrap_err();
        assert!(e.contains("device 1"), "{e}");
        assert_eq!((f.mode(0), f.mode(1)), ("Rainbow".into(), "Static".into()));
        assert!(!f.cfg.saved.exists());
        std::fs::remove_file(f.dir.join("name1")).unwrap();

        std::fs::write(f.dir.join("fail"), "").unwrap();
        assert!(sync(&f.cfg, Theme::Dark)
            .await
            .unwrap_err()
            .contains("boom"));
        let missing = Config {
            bin: f.dir.join("absent"),
            ..f.cfg.clone()
        };
        assert!(sync(&missing, Theme::Dark).await.is_err());
    }

    #[tokio::test]
    async fn worker_debounces_toggles_and_never_overlaps() {
        let f = Fake::new("worker");
        let (_stop_tx, stop) = watch::channel(false);
        let tx = spawn(&tokio::runtime::Handle::current(), f.cfg.clone(), stop);
        // Startup value, then a burst of toggles ending in dark.
        for t in [Theme::Light, Theme::Dark, Theme::Light, Theme::Dark] {
            request(&tx, t);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        request(&tx, Theme::Dark); // duplicate: ignored
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while f.mode(1) != OFF && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!((f.mode(0), f.mode(1)), (OFF.into(), OFF.into()));
        // One list + one set for the whole burst.
        assert_eq!(f.calls().len(), 2, "{:?}", f.calls());

        // Toggles while a sync runs queue behind it instead of overlapping.
        std::fs::write(f.dir.join("sleep"), "0.3").unwrap();
        request(&tx, Theme::Light);
        tokio::time::sleep(Duration::from_millis(300)).await;
        request(&tx, Theme::Dark);
        tokio::time::sleep(Duration::from_millis(300)).await;
        request(&tx, Theme::Light);
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while f.mode(1) != "Static" && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let calls = f.calls();
        assert!(!calls.iter().any(|c| c == "overlap"), "{calls:?}");
        assert_eq!(
            (f.mode(0), f.mode(1)),
            ("Rainbow".into(), "Static".into()),
            "{calls:?}"
        );
    }

    #[test]
    fn disabled_outside_the_users_own_state() {
        let mut vars = BTreeMap::new();
        vars.insert("HOME".to_string(), "/home/u".to_string());
        assert!(Config::from_env(&Env::from_vars(vars.clone())).is_some());
        for (k, v) in [
            ("XDG_STATE_HOME", "/tmp/x"),
            ("HYPRV_NESTED_TEST", "1"),
            ("HYPRV_NESTED_FIXTURE", "/f"),
            ("HYPRV_RAM_RGB", "off"),
        ] {
            let mut v2 = vars.clone();
            v2.insert(k.into(), v.into());
            assert!(Config::from_env(&Env::from_vars(v2)).is_none(), "{k}");
        }
        assert!(Config::from_env(&Env::rooted(Path::new("/tmp/r"))).is_none());
    }
}

/// Drives the real RAM LEDs (dark, then light) with a scratch save file:
/// `HYPRV_RAM_RGB_LIVE=1 cargo test -p hyprv-svc-desktop real_leds -- --ignored`.
#[cfg(test)]
#[tokio::test]
#[ignore]
async fn real_leds_round_trip() {
    if std::env::var_os("HYPRV_RAM_RGB_LIVE").is_none() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("hyprv-ram-rgb-live-{}", std::process::id()));
    let cfg = Config {
        saved: dir.join("ram-rgb.env"),
        lock: dir.join("lock"),
        ..Config::from_env(&Env::from_process()).expect("enabled")
    };
    let list = || async { parse_devices(&openrgb(&cfg, &["-ld".into()]).await.unwrap()) };
    let before = list().await;
    eprintln!(
        "before: {:?}",
        before
            .iter()
            .map(|d| (&d.name, &d.current))
            .collect::<Vec<_>>()
    );
    eprintln!("dark: {:?}", sync(&cfg, Theme::Dark).await.unwrap());
    let dark = list().await;
    assert!(dark[..2].iter().all(|d| d.current.as_deref() == Some(OFF)));
    assert_eq!(dark[2].current, before[2].current);
    eprintln!("light: {:?}", sync(&cfg, Theme::Light).await.unwrap());
    let after = list().await;
    for i in 0..3 {
        assert_eq!(after[i].current, before[i].current, "device {i}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
