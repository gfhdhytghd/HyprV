//! Exact ports of the awk editors in `apply-ui-state.sh` (`set_ini_value`,
//! `set_assignment`, `set_xsettings_value`) as pure text transforms, plus the
//! `ui-state.sh` parser/serializer.
use std::path::Path;

fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let t = text.strip_suffix('\n').unwrap_or(text);
    t.split('\n').collect()
}
fn join(out: Vec<String>) -> String {
    out.into_iter().map(|l| l + "\n").collect()
}

pub fn set_ini_value(text: &str, section: &str, key: &str, value: &str) -> String {
    let target = format!("[{section}]");
    let kv = format!("{key}={value}");
    let prefix = format!("{key}=");
    let (mut in_target, mut written) = (false, false);
    let mut out = Vec::new();
    let input = lines(text);
    for line in &input {
        if *line == target {
            if in_target && !written {
                out.push(kv.clone());
                written = true;
            }
            in_target = true;
            out.push(line.to_string());
            continue;
        }
        if line.starts_with('[') {
            if in_target && !written {
                out.push(kv.clone());
                written = true;
            }
            in_target = false;
            out.push(line.to_string());
            continue;
        }
        if in_target && line.starts_with(&prefix) {
            if !written {
                out.push(kv.clone());
                written = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if input.is_empty() {
        out.push(target);
        out.push(kv);
    } else if in_target && !written {
        out.push(kv);
    } else if !written {
        out.push(String::new());
        out.push(target);
        out.push(kv);
    }
    join(out)
}
pub fn set_assignment(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}=");
    let mut written = false;
    let mut out = Vec::new();
    for line in lines(text) {
        if line.starts_with(&prefix) {
            if !written {
                out.push(format!("{key}={value}"));
                written = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if !written {
        out.push(format!("{key}={value}"));
    }
    join(out)
}
/// awk `$1 == key`: first whitespace-separated field.
pub fn set_xsettings_value(text: &str, key: &str, value: &str) -> String {
    let mut written = false;
    let mut out = Vec::new();
    for line in lines(text) {
        if line.split_whitespace().next() == Some(key) {
            if !written {
                out.push(format!("{key} {value}"));
                written = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if !written {
        out.push(format!("{key} {value}"));
    }
    join(out)
}
/// Read-modify-write (create parents/file like `ensure_parent` + `touch`).
///
/// Only a confirmed missing file starts from empty; any other read or UTF-8
/// error aborts with the original untouched. The replacement keeps the file's
/// permissions and is a compare-and-swap against the bytes that were read
/// (re-applied on a concurrent change, then given up). Unchanged output is
/// not rewritten.
pub fn edit(path: &Path, f: impl Fn(&str) -> String) -> std::io::Result<()> {
    use crate::util::{is_conflict, replace_checked, Expect};
    for _ in 0..3 {
        let original = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let text = match &original {
            Some(bytes) => std::str::from_utf8(bytes).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: {e}", path.display()),
                )
            })?,
            None => "",
        };
        let next = f(text);
        if original.as_deref() == Some(next.as_bytes()) {
            return Ok(());
        }
        let expect = match &original {
            Some(bytes) => Expect::Bytes(bytes),
            None => Expect::Absent,
        };
        match replace_checked(path, next.as_bytes(), None, expect) {
            Err(e) if is_conflict(&e) => continue,
            other => return other,
        }
    }
    Err(std::io::Error::other(format!(
        "{} keeps changing; edit abandoned",
        path.display()
    )))
}

/// `ui-state.sh` `load_state` loop: last `theme=`/`variant=` wins.
pub fn parse_ui_state(text: &str) -> (Option<String>, Option<String>) {
    let (mut theme, mut variant) = (None, None);
    for line in text.lines() {
        let (k, v) = line.split_once('=').unwrap_or((line, ""));
        match k {
            "theme" => theme = Some(v.to_string()),
            "variant" => variant = Some(v.to_string()),
            _ => {}
        }
    }
    (theme, variant)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ini_semantics_match_awk() {
        assert_eq!(set_ini_value("", "Settings", "a", "1"), "[Settings]\na=1\n");
        assert_eq!(
            set_ini_value("[Settings]\na=0\nb=2\n[Other]\na=9\n", "Settings", "a", "1"),
            "[Settings]\na=1\nb=2\n[Other]\na=9\n"
        );
        assert_eq!(
            set_ini_value("[Settings]\nb=2", "Settings", "a", "1"),
            "[Settings]\nb=2\na=1\n"
        );
        assert_eq!(
            set_ini_value("[Settings]\nb=2\n[X]\n", "Settings", "a", "1"),
            "[Settings]\nb=2\na=1\n[X]\n"
        );
        assert_eq!(set_ini_value("x=1\n", "S", "a", "1"), "x=1\n\n[S]\na=1\n");
        // Duplicate keys collapse to the first position.
        assert_eq!(
            set_ini_value("[S]\na=0\na=5\n", "S", "a", "1"),
            "[S]\na=1\n"
        );
    }
    #[test]
    fn assignment_and_xsettings() {
        assert_eq!(
            set_assignment("a=1\nb=2\na=3\n", "a", "\"x\""),
            "a=\"x\"\nb=2\n"
        );
        assert_eq!(set_assignment("", "a", "1"), "a=1\n");
        assert_eq!(
            set_xsettings_value("Net/ThemeName \"A\"\nX 1\n", "Net/ThemeName", "\"B\""),
            "Net/ThemeName \"B\"\nX 1\n"
        );
        assert_eq!(set_xsettings_value("", "K", "1"), "K 1\n");
    }
    #[test]
    fn read_errors_preserve_bytes_and_mode_is_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("hyprv-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // DESKTOP-6: undecodable bytes abort the edit and stay byte-identical.
        let bad = dir.join("bad.ini");
        std::fs::write(&bad, [0xff, 0xfe, 0x00]).unwrap();
        assert!(edit(&bad, |t| set_ini_value(t, "Settings", "a", "1")).is_err());
        assert_eq!(std::fs::read(&bad).unwrap(), [0xff, 0xfe, 0x00]);
        // DESKTOP-7: a private file stays private under umask 022.
        let private = dir.join("private.ini");
        std::fs::write(&private, "[Settings]\nx=1\n").unwrap();
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o600)).unwrap();
        let old = unsafe { libc::umask(0o022) };
        edit(&private, |t| set_ini_value(t, "Settings", "a", "1")).unwrap();
        unsafe { libc::umask(old) };
        assert_eq!(
            std::fs::metadata(&private).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read_to_string(&private).unwrap(),
            "[Settings]\nx=1\na=1\n"
        );
        // Missing files are created.
        edit(&dir.join("new.ini"), |t| set_ini_value(t, "S", "k", "v")).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("new.ini")).unwrap(),
            "[S]\nk=v\n"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ui_state_parse() {
        assert_eq!(
            parse_ui_state("theme=dark\nvariant=v3\n"),
            (Some("dark".into()), Some("v3".into()))
        );
        assert_eq!(parse_ui_state("junk\n").0, None);
    }
}
