//! Main-shell light/dark source; Matugen is deliberately a separate topic.
use hyprv_core::{calloop::channel::Sender, config::FileWatch, tokio};
use std::{
    io,
    path::{Path, PathBuf},
};
pub fn path() -> io::Result<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .ok_or_else(|| io::Error::other("HOME/XDG_STATE_HOME missing"))?;
    Ok(base.join("hyprv/quickshell/ui-state.env"))
}
pub fn read_dark(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .any(|l| l.trim() == "theme=dark")
}
pub async fn watch(
    path: PathBuf,
    sender: Sender<bool>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> io::Result<()> {
    let mut last = None;
    loop {
        let watch = FileWatch::new(std::slice::from_ref(&path))?;
        let dark = read_dark(&path);
        if last != Some(dark) {
            if sender.send(dark).is_err() {
                return Ok(());
            }
            last = Some(dark);
        }
        if *shutdown.borrow() {
            return Ok(());
        }
        tokio::select! {result=watch.changed()=>result?,_=shutdown.changed()=>return Ok(())}
    }
}
