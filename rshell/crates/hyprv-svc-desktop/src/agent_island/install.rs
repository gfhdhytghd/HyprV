//! Explicit maintenance: port of `agent-island-install-hooks.py`. Never run at
//! startup. Same events, timeouts, matchers and Codex `[features] hooks = true`
//! edit; entries managed by either the Python or the Rust hook are replaced.
//!
//! Safety of the user's real files:
//! - installers serialize on a lock and every replace is a compare-and-swap
//!   against the bytes that were read; a concurrent edit is rebased (the
//!   managed-hook edit is re-applied onto the newer document) or reported;
//! - all three documents are read and transformed before anything is written;
//!   a read or parse error aborts without touching any file;
//! - `config.toml` is edited through a TOML syntax tree (`toml_edit`), keeping
//!   comments/order, and the result is re-parsed before it is written;
//! - hook paths are shell-quoted; managed entries are recognized by their exact
//!   argv shape, never by a substring;
//! - backups are versioned and created exclusively; file modes are preserved.
use super::ojson::{detect_indent, to_pretty, OValue};
use crate::Env;
use std::{
    io,
    path::{Path, PathBuf},
};

pub const CLAUDE_EVENTS: &[(&str, u32)] = &[
    ("UserPromptSubmit", 5),
    ("PreToolUse", 5),
    ("PostToolUse", 5),
    ("PostToolUseFailure", 5),
    ("PermissionRequest", 5),
    ("Stop", 5),
    ("SubagentStart", 5),
    ("SubagentStop", 5),
    ("SessionStart", 5),
    ("SessionEnd", 5),
    ("Notification", 5),
    ("PreCompact", 5),
];
pub const CODEX_EVENTS: &[(&str, u32)] = &[
    ("SessionStart", 5),
    ("SessionEnd", 5),
    ("UserPromptSubmit", 5),
    ("PreToolUse", 5),
    ("PostToolUse", 5),
    ("Stop", 5),
];
pub const CODEX_PASSTHROUGH_EVENTS: &[&str] = &["PermissionRequest"];

/// Which hook command to install. `Python` reproduces the legacy command exactly.
#[derive(Clone, Debug)]
pub enum HookCommand {
    Python { hook: PathBuf },
    Native { binary: PathBuf },
}
impl HookCommand {
    pub fn legacy(env: &Env) -> Self {
        Self::Python {
            hook: env
                .hyprv_config_dir
                .join("quickshell/scripts/agent-island-hook.py"),
        }
    }
    pub fn command_for(&self, source: &str) -> String {
        match self {
            Self::Python { hook } => format!(
                "/usr/bin/python3 {} --source {}",
                shell_quote(&hook.to_string_lossy()),
                shell_quote(source)
            ),
            Self::Native { binary } => format!(
                "{} --source {}",
                shell_quote(&binary.to_string_lossy()),
                shell_quote(source)
            ),
        }
    }
    fn target(&self) -> &Path {
        match self {
            Self::Python { hook } => hook,
            Self::Native { binary } => binary,
        }
    }
}

/// Python `shlex.quote`: unchanged when safe, else one single-quoted word.
pub fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@%+=:,./-_".contains(&b));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    }
}
/// POSIX-shell word splitting for plain commands. `None` when the command uses
/// anything beyond words and quoting (operators, expansions, substitutions),
/// i.e. it is not a simple invocation we could have written.
pub fn shell_split(s: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            ' ' | '\t' => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            '\'' => {
                has = true;
                loop {
                    match it.next()? {
                        '\'' => break,
                        ch => cur.push(ch),
                    }
                }
            }
            '"' => {
                has = true;
                loop {
                    match it.next()? {
                        '"' => break,
                        '\\' => match it.next()? {
                            ch @ ('"' | '\\' | '$' | '`') => cur.push(ch),
                            '\n' => {}
                            ch => {
                                cur.push('\\');
                                cur.push(ch);
                            }
                        },
                        '$' | '`' => return None,
                        ch => cur.push(ch),
                    }
                }
            }
            '\\' => {
                has = true;
                cur.push(it.next()?);
            }
            // Operators, expansions, globs, comments: not a plain invocation.
            ';' | '&' | '|' | '<' | '>' | '(' | ')' | '$' | '`' | '\n' | '*' | '?' | '[' => {
                return None
            }
            '#' | '~' if !has => return None,
            ch => {
                has = true;
                cur.push(ch);
            }
        }
    }
    if has {
        out.push(cur);
    }
    Some(out)
}
/// Managed = exactly the shape an installer writes:
/// `PATH/hyprv-agent-hook --source SRC` or
/// `PATH/python3 PATH/agent-island-hook.py --source SRC`.
pub fn is_managed(command: &str) -> bool {
    let Some(argv) = shell_split(command) else {
        return false;
    };
    let base = |s: &str| crate::util::basename(s).to_string();
    match argv.as_slice() {
        [exe, flag, _src] => base(exe) == "hyprv-agent-hook" && flag == "--source",
        [py, script, flag, _src] => {
            base(py).starts_with("python")
                && base(script) == "agent-island-hook.py"
                && flag == "--source"
        }
        _ => false,
    }
}
fn command_text(v: Option<&OValue>) -> String {
    v.map(OValue::text).unwrap_or_default()
}
pub fn remove_managed(entries: &[OValue]) -> Vec<OValue> {
    let mut kept = Vec::new();
    for entry in entries {
        if !entry.is_object() {
            kept.push(entry.clone());
            continue;
        }
        if let Some(OValue::Array(hooks)) = entry.get("hooks") {
            let hooks: Vec<OValue> = hooks
                .iter()
                .filter(|h| !(h.is_object() && is_managed(&command_text(h.get("command")))))
                .cloned()
                .collect();
            if !hooks.is_empty() {
                let mut copy = entry.clone();
                copy.insert("hooks", OValue::Array(hooks));
                kept.push(copy);
            }
            continue;
        }
        if !is_managed(&command_text(entry.get("command"))) {
            kept.push(entry.clone());
        }
    }
    kept
}
fn entries(hooks: &OValue, event: &str) -> Vec<OValue> {
    remove_managed(
        hooks
            .get(event)
            .and_then(OValue::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
    )
}
/// `[{"type": "command", "command": ..., "timeout": N}]` in Python key order.
fn hook_list(cmd: &HookCommand, source: &str, timeout: u32) -> OValue {
    OValue::Array(vec![OValue::Object(vec![
        ("type".into(), OValue::str("command")),
        ("command".into(), OValue::str(cmd.command_for(source))),
        ("timeout".into(), OValue::Number(timeout.into())),
    ])])
}
fn hooks_of(data: &OValue) -> OValue {
    data.get("hooks")
        .filter(|h| h.is_object())
        .cloned()
        .unwrap_or_else(OValue::obj)
}
pub fn install_claude_into(data: &mut OValue, cmd: &HookCommand) {
    let mut hooks = hooks_of(data);
    for (event, timeout) in CLAUDE_EVENTS {
        let mut list = entries(&hooks, event);
        list.push(OValue::Object(vec![
            ("matcher".into(), OValue::str("")),
            ("hooks".into(), hook_list(cmd, "claude", *timeout)),
        ]));
        hooks.insert(event, OValue::Array(list));
    }
    data.insert("hooks", hooks);
}
pub fn install_codex_into(data: &mut OValue, cmd: &HookCommand) {
    let mut hooks = hooks_of(data);
    for event in CODEX_PASSTHROUGH_EVENTS {
        let list = entries(&hooks, event);
        if list.is_empty() {
            hooks.remove(event);
        } else {
            hooks.insert(event, OValue::Array(list));
        }
    }
    for (event, timeout) in CODEX_EVENTS {
        let mut list = entries(&hooks, event);
        list.push(OValue::Object(vec![
            ("matcher".into(), OValue::str("*")),
            ("hooks".into(), hook_list(cmd, "codex", *timeout)),
            ("statusMessage".into(), OValue::str("HyprV island")),
        ]));
        hooks.insert(event, OValue::Array(list));
    }
    data.insert("hooks", hooks);
}
/// `enable_codex_hooks_feature` on a TOML syntax tree: ensures
/// `[features] hooks = true` (a lone legacy `codex_hooks` key is replaced, as the
/// line editor did), preserving comments, order and formatting elsewhere.
/// Invalid input, or a `features` that is not a table, is an error.
pub fn enable_codex_feature(text: &str) -> Result<String, String> {
    use toml_edit::{DocumentMut, Item, Table, Value};
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("config.toml is not valid TOML: {e}"))?;
    if !doc.contains_key("features") {
        doc.insert("features", Item::Table(Table::new()));
    }
    let features = doc["features"]
        .as_table_like_mut()
        .ok_or("config.toml: `features` is not a table")?;
    if features.get("hooks").is_none() {
        features.remove("codex_hooks");
    }
    match features.get_mut("hooks") {
        Some(item) => match item.as_value_mut() {
            Some(v) if v.as_bool() == Some(true) => {}
            Some(v) => {
                let decor = v.decor().clone();
                *v = Value::from(true);
                *v.decor_mut() = decor;
            }
            None => return Err("config.toml: `features.hooks` is not a value".into()),
        },
        None => {
            features.insert("hooks", toml_edit::value(true));
        }
    }
    let out = doc.to_string();
    // Validate before anything is written.
    out.parse::<DocumentMut>()
        .map_err(|e| format!("refusing to write invalid TOML: {e}"))?;
    Ok(out)
}

type Transform<'a> = &'a dyn Fn(Option<&[u8]>) -> io::Result<String>;

/// Missing = `None`; every other read error aborts the installation.
fn read_opt(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io::Error::new(e.kind(), format!("{}: {e}", path.display()))),
    }
}
fn utf8<'a>(path: &Path, bytes: &'a [u8]) -> io::Result<&'a str> {
    std::str::from_utf8(bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {e}", path.display()),
        )
    })
}
/// Order/format-preserving JSON edit of one document.
fn json_edit(path: &Path, bytes: Option<&[u8]>, f: &dyn Fn(&mut OValue)) -> io::Result<String> {
    let (mut value, indent, newline) = match bytes {
        None => (OValue::obj(), "  ".to_string(), true),
        Some(b) => {
            let text = utf8(path, b)?;
            match serde_json::from_str::<OValue>(text) {
                Ok(v) if v.is_object() => (
                    v,
                    detect_indent(text),
                    text.ends_with('\n') || text.is_empty(),
                ),
                Ok(_) => {
                    return Err(io::Error::other(format!(
                        "JSON root must be an object: {}",
                        path.display()
                    )))
                }
                Err(_) => {
                    return Err(io::Error::other(format!(
                        "Cannot parse JSON: {}",
                        path.display()
                    )))
                }
            }
        }
    };
    f(&mut value);
    Ok(to_pretty(&value, &indent, newline))
}
fn toml_edit_doc(path: &Path, bytes: Option<&[u8]>) -> io::Result<String> {
    let text = match bytes {
        Some(b) => utf8(path, b)?,
        None => "",
    };
    enable_codex_feature(text).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {e}", path.display()),
        )
    })
}
/// Commit one planned document: versioned backup, then compare-and-swap against
/// the bytes the plan was computed from. A concurrent change is rebased (the
/// edit is recomputed from the newer document) a few times, then reported.
/// Returns whether the file was written.
fn commit(
    path: &Path,
    mut original: Option<Vec<u8>>,
    mut next: String,
    transform: Transform<'_>,
) -> io::Result<bool> {
    use crate::util::{is_conflict, replace_checked, versioned_backup, Expect};
    for _ in 0..5 {
        if original.as_deref() == Some(next.as_bytes()) {
            return Ok(false);
        }
        let backup = match &original {
            Some(bytes) => {
                let mode = std::fs::symlink_metadata(path)
                    .map(|m| {
                        use std::os::unix::fs::PermissionsExt;
                        m.permissions().mode() & 0o7777
                    })
                    .unwrap_or(0o600);
                Some(versioned_backup(path, bytes, mode)?)
            }
            None => None,
        };
        let expect = match &original {
            Some(b) => Expect::Bytes(b),
            None => Expect::Absent,
        };
        match replace_checked(path, next.as_bytes(), None, expect) {
            Ok(()) => return Ok(true),
            Err(e) if is_conflict(&e) => {
                // Our own backup of the superseded version is no longer needed.
                if let Some(b) = backup {
                    let _ = std::fs::remove_file(b);
                }
                original = read_opt(path)?;
                next = transform(original.as_deref())?;
            }
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other(format!(
        "{} keeps changing; installation stopped",
        path.display()
    )))
}
fn codex_home(env: &Env) -> PathBuf {
    env.var("CODEX_HOME")
        .map(|p| crate::util::expand_tilde(Path::new(p), &env.home))
        .unwrap_or_else(|| env.home.join(".codex"))
}
/// Returns the legacy stdout lines (`installed=claude,codex`, `hook=PATH`).
pub fn install(env: &Env, cmd: &HookCommand) -> io::Result<String> {
    install_with(env, cmd, &|_| {})
}
/// `before_commit` runs after every document was read and transformed and
/// before anything is written (tests use it to inject concurrent edits).
pub fn install_with(
    env: &Env,
    cmd: &HookCommand,
    before_commit: &dyn Fn(&[PathBuf]),
) -> io::Result<String> {
    if !cmd.target().exists() {
        return Err(io::Error::other(format!(
            "Missing hook script: {}",
            cmd.target().display()
        )));
    }
    // Cooperating installers are serialized; foreign writers are caught by CAS.
    let _lock = crate::util::FileLock::exclusive_blocking(
        &env.cache_home.join("hyprv/agent-hook-install.lock"),
    )?;
    let codex = codex_home(env);
    let claude_path = env.home.join(".claude/settings.json");
    let hooks_path = codex.join("hooks.json");
    let config_path = codex.join("config.toml");
    let claude_t = |b: Option<&[u8]>| json_edit(&claude_path, b, &|d| install_claude_into(d, cmd));
    let hooks_t = |b: Option<&[u8]>| json_edit(&hooks_path, b, &|d| install_codex_into(d, cmd));
    let config_t = |b: Option<&[u8]>| toml_edit_doc(&config_path, b);
    let targets: [(&PathBuf, Transform<'_>); 3] = [
        (&claude_path, &claude_t),
        (&hooks_path, &hooks_t),
        (&config_path, &config_t),
    ];
    // Preflight: read and transform everything; any error aborts with no writes.
    let mut plans = Vec::new();
    for (path, transform) in targets {
        let original = read_opt(path)?;
        let next = transform(original.as_deref())?;
        plans.push((path, original, next, transform));
    }
    before_commit(&[claude_path.clone(), hooks_path.clone(), config_path.clone()]);
    for (path, original, next, transform) in plans {
        commit(path, original, next, transform)?;
    }
    Ok(format!(
        "installed=claude,codex\nhook={}\n",
        cmd.target().display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn parse(t: &str) -> OValue {
        serde_json::from_str(t).unwrap()
    }
    fn plain(v: &OValue) -> Value {
        serde_json::to_value(v).unwrap()
    }
    #[test]
    fn replaces_only_managed_entries() {
        let cmd = HookCommand::Native {
            binary: "/b/hyprv-agent-hook".into(),
        };
        let mut data = parse(
            r#"{"other": 1, "hooks":{"Stop":[
            {"matcher":"","hooks":[{"type":"command","command":"/usr/bin/python3 /x/agent-island-hook.py --source claude"},
                                   {"type":"command","command":"notify"}]}]}, "z": 2}"#,
        );
        install_claude_into(&mut data, &cmd);
        let v = plain(&data);
        let stop = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(
            stop[1]["hooks"][0]["command"],
            "/b/hyprv-agent-hook --source claude"
        );
        assert_eq!(stop[1]["hooks"][0]["timeout"], 5);
        assert_eq!(v["hooks"].as_object().unwrap().len(), 12);
        // Order: top-level keys keep position; Stop keeps its slot; new events append in source order.
        let OValue::Object(top) = &data else { panic!() };
        assert_eq!(
            top.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["other", "hooks", "z"]
        );
        let OValue::Object(h) = data.get("hooks").unwrap() else {
            panic!()
        };
        assert_eq!(h[0].0, "Stop");
        assert_eq!(h[1].0, "UserPromptSubmit");
        let text = to_pretty(&data, "  ", true);
        let (t, c, o) = (
            text.find("\"type\": \"command\"").unwrap(),
            text.find("\"command\": \"/b/").unwrap(),
            text.rfind("\"timeout\": 5").unwrap(),
        );
        assert!(
            t < c && c < o,
            "python key order type/command/timeout: {text}"
        );
        let again = {
            let mut d = parse(&text);
            install_claude_into(&mut d, &cmd);
            to_pretty(&d, "  ", true)
        };
        assert_eq!(again, text, "reinstall is byte-identical");
    }
    fn backups(dir: &Path, prefix: &str) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix))
            .collect()
    }
    fn temp_home(tag: &str) -> (PathBuf, Env, HookCommand) {
        let root = std::env::temp_dir().join(format!("hyprv-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let env = Env::rooted(&root);
        let hook = root.join("hyprv-agent-hook");
        std::fs::write(&hook, "").unwrap();
        std::fs::create_dir_all(env.home.join(".claude")).unwrap();
        std::fs::create_dir_all(env.home.join(".codex")).unwrap();
        (root, env, HookCommand::Native { binary: hook })
    }
    #[test]
    fn codex_hooks_and_feature() {
        let cmd = HookCommand::Python {
            hook: "/h/agent-island-hook.py".into(),
        };
        let mut data = parse(
            r#"{"hooks":{"PermissionRequest":[{"hooks":[{"command":"python3 /h/agent-island-hook.py --source codex"}]}]}}"#,
        );
        install_codex_into(&mut data, &cmd);
        let v = plain(&data);
        assert!(v["hooks"].get("PermissionRequest").is_none());
        assert_eq!(v["hooks"]["Stop"][0]["statusMessage"], "HyprV island");
        assert_eq!(v["hooks"]["Stop"][0]["matcher"], "*");
        assert_eq!(
            enable_codex_feature("").unwrap(),
            "[features]\nhooks = true\n"
        );
        assert_eq!(
            enable_codex_feature("a = 1\n").unwrap(),
            "a = 1\n\n[features]\nhooks = true\n"
        );
        assert_eq!(
            enable_codex_feature("[features]\ncodex_hooks = false\n[x]\n").unwrap(),
            "[features]\nhooks = true\n[x]\n"
        );
        assert_eq!(
            enable_codex_feature("[features]\nother = 1\n[x]\ny=2").unwrap(),
            "[features]\nother = 1\nhooks = true\n[x]\ny=2\n"
        );
        let already = "[features] # mine\nhooks = true # keep\n";
        assert_eq!(enable_codex_feature(already).unwrap(), already);
    }
    /// DESKTOP-3: comments, quoted keys, multiline strings and dotted/inline
    /// tables stay valid single-table TOML; invalid input is refused.
    #[test]
    fn toml_edits_preserve_structure_and_validity() {
        use toml_edit::DocumentMut;
        let cases = [
            "[features] # personal switches\nother = 1\n",
            "[\"features\"]\nother = 1\n",
            "x = '''\n[features]\n'''\n[features]\nother = 1\n",
            "features.other = 1\n",
            "features = { other = 1 }\n",
            "# head\nmodel = \"o3\" # trailing\n\n[profiles.a]\nk = 1\n",
            "[features]\nhooks = false # was off\n",
        ];
        for input in cases {
            let out = enable_codex_feature(input).unwrap();
            let doc: DocumentMut = out
                .parse()
                .unwrap_or_else(|e| panic!("{input:?} -> {out:?}: {e}"));
            assert_eq!(doc["features"]["hooks"].as_bool(), Some(true), "{out}");
            let headers = |t: &str| {
                t.lines()
                    .filter(|l| l.starts_with("[features]") || l.starts_with("[\"features\"]"))
                    .count()
            };
            // Never adds a second header when the input already declares the table.
            assert!(
                headers(&out) <= headers(input).max(1),
                "duplicate table: {out}"
            );
            for comment in ["# personal switches", "# head", "# trailing", "# was off"] {
                if input.contains(comment) {
                    assert!(out.contains(comment), "comment lost: {out}");
                }
            }
        }
        let multi =
            enable_codex_feature("x = '''\n[features]\n'''\n[features]\nother = 1\n").unwrap();
        assert!(
            multi.starts_with("x = '''\n[features]\n'''\n"),
            "multiline string untouched: {multi}"
        );
        assert!(enable_codex_feature("[features\n").is_err());
        assert!(enable_codex_feature("features = 3\n").is_err());
        // The real config round-trips byte-identically (read-only).
        if let Ok(real) = std::fs::read_to_string(
            Path::new(&std::env::var("HOME").unwrap_or_default()).join(".codex/config.toml"),
        ) {
            let doc: DocumentMut = real.parse().unwrap();
            assert_eq!(doc.to_string(), real);
        }
    }
    /// DESKTOP-4: hostile paths stay one shell word; executing the command in a
    /// scratch dir must not run anything injected.
    #[test]
    fn hook_paths_are_one_shell_argument() {
        for path in [
            "/opt/my tools/hyprv-agent-hook",
            "/x/it's/hyprv-agent-hook",
            "/x/a\"b/hyprv-agent-hook",
            "/x/$(touch pwned)/hyprv-agent-hook",
            "/x/`id`/hyprv-agent-hook",
            "/x/new\nline/hyprv-agent-hook",
            "/x/hyprv-agent-hook;printf injected>injected-marker #",
        ] {
            for cmd in [
                HookCommand::Native {
                    binary: path.into(),
                },
                HookCommand::Python { hook: path.into() },
            ] {
                let line = cmd.command_for("claude");
                let argv = shell_split(&line).unwrap_or_else(|| panic!("unparseable: {line}"));
                let got = match cmd {
                    HookCommand::Native { .. } => &argv[0],
                    HookCommand::Python { .. } => &argv[1],
                };
                assert_eq!(got, path, "{line}");
                assert_eq!(argv.last().unwrap(), "claude");
            }
        }
        // Execute the generated line under /bin/sh in a scratch dir.
        let dir = std::env::temp_dir().join(format!("hyprv-quote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let hostile = dir.join("hyprv-agent-hook;printf injected>injected-marker #");
        std::fs::write(&hostile, "").unwrap();
        let line = HookCommand::Native { binary: hostile }.command_for("claude");
        let _ = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&line)
            .current_dir(&dir)
            .output();
        assert!(
            !dir.join("injected-marker").exists(),
            "shell injection via hook path: {line}"
        );
        std::fs::remove_dir_all(dir).unwrap();
        // Ordinary paths keep the exact legacy text.
        assert_eq!(
            HookCommand::Python {
                hook: "/home/u/.config/HyprV/quickshell/scripts/agent-island-hook.py".into()
            }
            .command_for("claude"),
            "/usr/bin/python3 /home/u/.config/HyprV/quickshell/scripts/agent-island-hook.py --source claude"
        );
    }
    /// DESKTOP-5: only the exact installed invocation shapes are managed.
    #[test]
    fn managed_detection_is_exact() {
        assert!(is_managed(
            "/usr/bin/python3 /home/u/q/agent-island-hook.py --source claude"
        ));
        assert!(is_managed(
            "python3 '/a b/agent-island-hook.py' --source codex"
        ));
        assert!(is_managed(
            "'/opt/my tools/hyprv-agent-hook' --source claude"
        ));
        for keep in [
            "logger 'hyprv-agent-hook failed'",
            "echo agent-island-hook.py --source x",
            "/usr/bin/python3 /x/agent-island-hook.py --source claude; rm -rf ~",
            "/usr/bin/python3 /x/agent-island-hook.py --source claude --extra",
            "notify-send hyprv-agent-hook",
            "/x/hyprv-agent-hook-wrapper --source claude",
        ] {
            assert!(!is_managed(keep), "{keep}");
        }
        let entries = vec![parse(
            r#"{"matcher":"","hooks":[{"type":"command","command":"logger 'hyprv-agent-hook failed'"}]}"#,
        )];
        assert_eq!(remove_managed(&entries), entries);
    }
    /// DESKTOP-1: an edit landing between read and write is rebased, not lost.
    #[test]
    fn concurrent_settings_edit_is_preserved() {
        let (root, env, cmd) = temp_home("cas");
        let settings = env.home.join(".claude/settings.json");
        std::fs::write(
            &settings,
            "{\n  \"permissions\": {\n    \"old\": true\n  }\n}\n",
        )
        .unwrap();
        install_with(&env, &cmd, &|_| {
            // Another editor commits a newer document after our preflight.
            std::fs::write(
                &settings,
                "{\n  \"permissions\": {\n    \"new\": true\n  }\n}\n",
            )
            .unwrap();
        })
        .unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(
            v["permissions"],
            serde_json::json!({"new": true}),
            "concurrent edit kept"
        );
        assert_eq!(
            v["hooks"].as_object().unwrap().len(),
            12,
            "managed hooks rebased on top"
        );
        let baks = backups(&env.home.join(".claude"), "settings.json.hyprv-bak.");
        assert_eq!(
            baks.len(),
            1,
            "one backup, of the version actually replaced"
        );
        assert!(std::fs::read_to_string(&baks[0])
            .unwrap()
            .contains("\"new\""));
        std::fs::remove_dir_all(root).unwrap();
    }
    /// Read/parse errors anywhere abort before any file is written.
    #[test]
    fn preflight_errors_write_nothing() {
        let (root, env, cmd) = temp_home("preflight");
        let settings = env.home.join(".claude/settings.json");
        let config = env.home.join(".codex/config.toml");
        std::fs::write(&settings, "{\"a\": 1}\n").unwrap();
        std::fs::write(&config, "[features\n").unwrap();
        assert!(install(&env, &cmd).is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{\"a\": 1}\n");
        assert!(!env.home.join(".codex/hooks.json").exists());
        std::fs::write(&config, "").unwrap();
        std::fs::write(&settings, [0xff, 0xfe]).unwrap();
        assert!(install(&env, &cmd).is_err());
        assert_eq!(std::fs::read(&settings).unwrap(), [0xff, 0xfe]);
        assert!(!env.home.join(".codex/hooks.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn install_in_temp_home_writes_once_then_never() {
        use std::os::unix::fs::PermissionsExt;
        let (root, env, cmd) = temp_home("once");
        let settings = env.home.join(".claude/settings.json");
        std::fs::write(
            &settings,
            "{\n    \"permissions\": {\n        \"x\": 1\n    }\n}",
        )
        .unwrap();
        std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o640)).unwrap();
        // A stale backup symlink must never be followed (DESKTOP-2).
        let victim = root.join("victim");
        std::fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(&victim, env.home.join(".claude/settings.json.hyprv-bak"))
            .unwrap();
        install(&env, &cmd).unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        let first = std::fs::read_to_string(&settings).unwrap();
        assert!(
            first.starts_with("{\n    \"permissions\": {\n        \"x\": 1\n    },\n    \"hooks\""),
            "{first}"
        );
        assert!(!first.ends_with('\n'), "trailing-newline style preserved");
        assert_eq!(
            std::fs::metadata(&settings).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(
            backups(&env.home.join(".claude"), "settings.json.hyprv-bak.").len(),
            1
        );
        let mtime = std::fs::metadata(&settings).unwrap().modified().unwrap();
        install(&env, &cmd).unwrap();
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), first);
        assert_eq!(
            std::fs::metadata(&settings).unwrap().modified().unwrap(),
            mtime,
            "no rewrite"
        );
        assert_eq!(
            backups(&env.home.join(".claude"), "settings.json.hyprv-bak.").len(),
            1,
            "no backup when unchanged"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
