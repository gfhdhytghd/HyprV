//! FR-NET-1: a writer that lands between the installer's last content check and
//! its replacement must not lose its edit. An `LD_PRELOAD` shim interposes
//! `renameat2`/`renameat` and writes a concurrent user edit into the target
//! just before the installer's swap (and optionally again before the swap
//! back). Only a scratch HOME / CODEX_HOME / XDG tree is used.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

const SHIM: &str = r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
static int calls;
static void inject(const char *newpath) {
    const char *target = getenv("RACE_TARGET");
    const char *base = target ? strrchr(target, '/') : NULL;
    if (!base || strcmp(newpath, base + 1) != 0) return;
    calls++;
    const char *content = NULL;
    if (calls == 1) content = getenv("RACE_FIRST");
    if (calls == 2) content = getenv("RACE_SECOND");
    if (!content) return;
    int fd = open(target, O_WRONLY | O_TRUNC);
    if (fd < 0) return;
    if (write(fd, content, strlen(content)) < 0) {}
    close(fd);
    fprintf(stderr, "race-shim: injected before rename %d\n", calls);
}
int renameat2(int od, const char *op, int nd, const char *np, unsigned int flags) {
    static int (*real)(int, const char *, int, const char *, unsigned int);
    if (!real) real = dlsym(RTLD_NEXT, "renameat2");
    inject(np);
    return real(od, op, nd, np, flags);
}
int renameat(int od, const char *op, int nd, const char *np) {
    static int (*real)(int, const char *, int, const char *);
    if (!real) real = dlsym(RTLD_NEXT, "renameat");
    inject(np);
    return real(od, op, nd, np);
}
"#;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hyprv-hook-race-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("home/.claude")).unwrap();
    std::fs::create_dir_all(d.join("codex")).unwrap();
    d
}
fn shim(dir: &Path) -> Option<PathBuf> {
    let src = dir.join("race.c");
    let so = dir.join("race.so");
    std::fs::write(&src, SHIM).unwrap();
    let ok = Command::new("cc")
        .args(["-shared", "-fPIC", "-O1", "-o"])
        .arg(&so)
        .arg(&src)
        .arg("-ldl")
        .status()
        .is_ok_and(|s| s.success());
    ok.then_some(so)
}
struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
    settings: String,
    siblings: Vec<(String, String)>,
}
fn install(dir: &Path, preload: Option<&Path>, first: Option<&str>, second: Option<&str>) -> Run {
    let settings = dir.join("home/.claude/settings.json");
    std::fs::write(&settings, "{\"original\": true}\n").unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hyprv-agent-hook"));
    cmd.arg("install")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", dir.join("home"))
        .env("CODEX_HOME", dir.join("codex"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_RUNTIME_DIR", dir.join("run"))
        .env("RACE_TARGET", &settings);
    if let Some(so) = preload {
        cmd.env("LD_PRELOAD", so);
    }
    if let Some(c) = first {
        cmd.env("RACE_FIRST", c);
    }
    if let Some(c) = second {
        cmd.env("RACE_SECOND", c);
    }
    let out = cmd.output().unwrap();
    let mut siblings: Vec<(String, String)> = std::fs::read_dir(dir.join("home/.claude"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "settings.json")
        .map(|n| {
            let body =
                std::fs::read_to_string(dir.join("home/.claude").join(&n)).unwrap_or_default();
            (n, body)
        })
        .collect();
    siblings.sort();
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        settings: std::fs::read_to_string(&settings).unwrap(),
        siblings,
    }
}
fn has_hook(settings: &str) -> bool {
    settings.contains("hyprv-agent-hook")
}

#[test]
fn concurrent_edit_in_the_final_window_survives_and_is_backed_up() {
    let dir = scratch("final-window");
    let Some(so) = shim(&dir) else {
        eprintln!("no C compiler; skipped");
        return;
    };
    // Control: no injection.
    let run = install(&dir, Some(&so), None, None);
    assert!(run.ok, "{}", run.stderr);
    assert!(has_hook(&run.settings) && run.settings.contains("original"));

    // A user edit lands just before the installer's swap.
    let edit = "{\"original\": true, \"concurrent_user_setting\": \"must-survive\"}\n";
    let run = install(&dir, Some(&so), Some(edit), None);
    assert!(
        run.stderr.contains("injected before rename 1"),
        "shim did not fire: {}",
        run.stderr
    );
    assert!(run.ok, "{}", run.stderr);
    assert!(run.stdout.contains("installed=claude,codex"));
    assert!(
        run.settings.contains("must-survive"),
        "concurrent edit lost: {}",
        run.settings
    );
    assert!(
        has_hook(&run.settings),
        "edit not rebased: {}",
        run.settings
    );
    // The generation the successful swap displaced is the user's edit, kept as
    // the backup.
    assert!(
        run.siblings
            .iter()
            .any(|(n, b)| n.contains(".hyprv-bak.") && b.contains("must-survive")),
        "displaced generation not kept: {:?}",
        run.siblings
    );
    assert!(
        !run.siblings
            .iter()
            .any(|(n, _)| n.starts_with(".settings.json.")),
        "temp file left behind: {:?}",
        run.siblings
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_second_writer_during_the_swap_back_is_kept_not_deleted() {
    let dir = scratch("swap-back");
    let Some(so) = shim(&dir) else {
        eprintln!("no C compiler; skipped");
        return;
    };
    let first = "{\"original\": true, \"first_writer\": 1}\n";
    // Written into the file that is the target right before the swap back,
    // i.e. into the installer's version: a third generation.
    let second = "{\"original\": true, \"second_writer\": 2}\n";
    let run = install(&dir, Some(&so), Some(first), Some(second));
    assert!(
        run.stderr.contains("injected before rename 2"),
        "{}",
        run.stderr
    );
    assert!(run.ok, "{}", run.stderr);
    assert!(has_hook(&run.settings), "{}", run.settings);
    assert!(run.settings.contains("first_writer"), "{}", run.settings);
    assert!(
        run.siblings
            .iter()
            .any(|(n, b)| n.contains(".hyprv-conflict.") && b.contains("second_writer")),
        "third generation was deleted: {:?}",
        run.siblings
    );
    std::fs::remove_dir_all(dir).unwrap();
}
