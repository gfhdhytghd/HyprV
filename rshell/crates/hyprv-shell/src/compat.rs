//! Argument compatibility only; never invokes or replaces a live Quickshell.
/// Accept the two legacy `-p/--path` IPC orders. The deployment patch points the
/// laptop binds at this executable; no global `quickshell` executable is replaced.
pub fn normalize_args(args: Vec<String>) -> Result<Vec<String>, String> {
    if !args.iter().any(|a| matches!(a.as_str(), "-p" | "--path")) {
        return Ok(args);
    }
    let mut out = Vec::new();
    let mut path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if matches!(arg.as_str(), "-p" | "--path") {
            path = Some(iter.next().ok_or("missing Quickshell compatibility path")?);
        } else {
            out.push(arg);
        }
    }
    if out.first().is_some_and(|a| a == "ipc") {
        return Ok(out);
    }
    if out.is_empty()
        && path.as_deref().is_some_and(|p| {
            std::path::Path::new(p)
                .file_name()
                .is_some_and(|p| p == "StartupConfirm.qml")
        })
    {
        return Ok(vec!["startup-confirm".into()]);
    }
    Err("legacy path arguments are supported only for ipc call or StartupConfirm.qml".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_legacy_ipc_orders_preserve_payloads() {
        for a in [
            vec![
                "ipc",
                "-p",
                "/config/quickshell",
                "call",
                "bar",
                "toggle",
                "eDP-1",
            ],
            vec![
                "-p",
                "/config/quickshell",
                "ipc",
                "call",
                "bar",
                "toggle",
                "eDP-1",
            ],
        ] {
            assert_eq!(
                normalize_args(a.into_iter().map(str::to_owned).collect()).unwrap(),
                ["ipc", "call", "bar", "toggle", "eDP-1"]
            );
        }
        assert_eq!(
            normalize_args(vec!["--path".into(), "/config/StartupConfirm.qml".into()]).unwrap(),
            ["startup-confirm"]
        );
        assert!(normalize_args(vec!["-p".into()]).is_err());
    }
}
