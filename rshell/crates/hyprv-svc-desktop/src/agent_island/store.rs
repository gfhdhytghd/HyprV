//! Locked `state.json` access, compatible with the Python hooks
//! (`state.lock` flock, compact JSON plus newline, atomic replace).
use super::model::{normalize_state, STATE_VERSION};
use crate::{util, Env};
use serde_json::{json, Value};
use std::{
    io,
    path::{Path, PathBuf},
};

pub fn state_dir(env: &Env) -> PathBuf {
    let raw = env
        .var("HYPRV_AGENT_ISLAND_DIR")
        .unwrap_or("~/.cache/hyprv/agent-island");
    util::expand_tilde(Path::new(raw), &env.home)
}
pub fn ensure_dirs(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir.join("responses"))
}
pub fn read_state(dir: &Path, now: f64) -> Value {
    let data = std::fs::read(dir.join("state.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    normalize_state(data, now)
}
pub fn write_state(dir: &Path, state: &mut Value, now: f64) -> io::Result<()> {
    if let Some(o) = state.as_object_mut() {
        o.insert("version".into(), json!(STATE_VERSION));
        o.insert("updated_at".into(), json!(now));
    }
    let mut text = serde_json::to_string(state).map_err(io::Error::other)?;
    text.push('\n');
    util::atomic_write(&dir.join("state.json"), text.as_bytes(), None)
}
/// Run `f` with the state locked; writes back when `f` returns true.
pub fn with_locked<R>(
    dir: &Path,
    now: f64,
    f: impl FnOnce(&mut Value) -> (bool, R),
) -> io::Result<R> {
    ensure_dirs(dir)?;
    let _lock = util::FileLock::exclusive_blocking(&dir.join("state.lock"))?;
    let mut state = read_state(dir, now);
    let (write, result) = f(&mut state);
    if write {
        write_state(dir, &mut state, now)?;
    }
    Ok(result)
}
