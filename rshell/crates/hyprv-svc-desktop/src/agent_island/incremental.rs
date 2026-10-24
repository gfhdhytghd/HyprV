//! Read-only fast path for `state.json` refreshes.
//!
//! Hooks (and the legacy observer) rewrite the whole file for every event, but
//! usually only one or two sessions change. The top level is parsed into
//! borrowed `RawValue`s (validation only, no allocation per node); a session
//! is parsed into a `Value` only when its raw text differs from the cached copy.
//! Cleanup/expiry/view use the same per-session functions as the full path
//! (`model::session_survives`, `repair_session`, `public_row`, ...).
use super::model::{self, Obj};
use serde::Deserialize;
use serde_json::{value::RawValue, Value};
use std::{borrow::Cow, collections::HashMap};

#[derive(Deserialize)]
struct Top<'a> {
    #[serde(borrow, default)]
    sessions: Option<HashMap<Cow<'a, str>, &'a RawValue>>,
    #[serde(borrow, default)]
    pending_requests: Option<&'a RawValue>,
}
struct Cached {
    raw: String,
    session: Option<Obj>,
    /// Display row of a session that needs no time-dependent repair.
    row: Option<model::PublicSession>,
}
impl Cached {
    fn new(raw: &str) -> Self {
        let session = match serde_json::from_str::<Value>(raw) {
            Ok(Value::Object(o)) => Some(o),
            _ => None,
        };
        let row = session
            .as_ref()
            .filter(|s| !needs_repair(s))
            .and_then(model::public_row);
        Self {
            raw: raw.to_string(),
            session,
            row,
        }
    }
}
#[derive(Default)]
pub struct Incremental {
    cache: std::collections::BTreeMap<String, Cached>,
    last_bytes: Vec<u8>,
    /// Session keys in document order (legacy dict order).
    keys: Vec<String>,
    pending_raw: Option<String>,
    /// Sessions parsed on the last refresh (for tests/diagnostics).
    pub parsed_last: usize,
}
pub struct View {
    pub rows: Vec<model::PublicSession>,
    pub pending: usize,
    pub expiry: Option<f64>,
    pub pids: Vec<i64>,
    /// Surviving file sessions in file order, then overlay-only ones.
    pub file_live: Vec<String>,
    pub overlay_live: Vec<String>,
}
impl Incremental {
    /// `None` when the document is not in the expected shape (caller falls back
    /// to the full normalizing path).
    pub fn refresh(
        &mut self,
        bytes: &[u8],
        overlay: &mut model::Obj,
        now: f64,
        alive: &mut dyn FnMut(i64) -> bool,
    ) -> Option<View> {
        self.parsed_last = 0;
        // Identical bytes (e.g. a pid-exit or expiry wake): the cache is current.
        if bytes != self.last_bytes.as_slice() {
            let top: Top = if bytes.is_empty() {
                Top {
                    sessions: None,
                    pending_requests: None,
                }
            } else {
                serde_json::from_slice(bytes).ok()?
            };
            let sessions = top.sessions.unwrap_or_default();
            self.cache
                .retain(|sid, _| sessions.contains_key(sid.as_str()));
            for (sid, raw) in &sessions {
                let text = raw.get();
                match self.cache.get_mut(sid.as_ref()) {
                    Some(c) if c.raw == text => {}
                    Some(c) => {
                        self.parsed_last += 1;
                        *c = Cached::new(text);
                    }
                    None => {
                        self.parsed_last += 1;
                        self.cache.insert(sid.to_string(), Cached::new(text));
                    }
                }
            }
            self.pending_raw = top.pending_requests.map(|r| r.get().to_string());
            self.keys = model::session_keys(bytes);
            self.last_bytes.clear();
            self.last_bytes.extend_from_slice(bytes);
        }
        let mut rows = Vec::with_capacity(self.cache.len());
        let mut expiry: Option<f64> = None;
        let mut pids = Vec::new();
        let mut live: std::collections::HashSet<&str> = std::collections::HashSet::new();
        // Same order as the full path: cleanup on the stored session, then
        // repair a copy for display/expiry (repairs are never persisted here).
        let mut visit = |session: &Obj, cached_row: Option<&model::PublicSession>| -> bool {
            if !model::session_survives(session, now, alive) {
                return false;
            }
            let repaired;
            let shown = if needs_repair(session) {
                let mut copy = session.clone();
                model::repair_session(&mut copy, now);
                repaired = copy;
                &repaired
            } else {
                session
            };
            if let Some(pid) = model::session_pid(shown).filter(|p| *p > 0) {
                pids.push(pid);
            }
            if let Some(t) = model::session_expiry(shown, alive) {
                expiry = Some(expiry.map_or(t, |e: f64| e.min(t)));
            }
            match cached_row {
                Some(row) => rows.push(row.clone()),
                None => rows.extend(model::public_row(shown)),
            }
            true
        };
        for (sid, cached) in &self.cache {
            if let Some(session) = &cached.session {
                if visit(session, cached.row.as_ref()) {
                    live.insert(sid.as_str());
                }
            }
        }
        let mut overlay_live = Vec::new();
        overlay.retain(|sid, v| {
            if self.cache.contains_key(sid) {
                return true; // the file's copy wins (or_insert semantics)
            }
            let keep = v.as_object().is_some_and(|o| visit(o, None));
            if keep {
                overlay_live.push(sid.clone());
            }
            keep
        });
        live.extend(overlay_live.iter().map(String::as_str));
        let mut pending = 0;
        if let Some(raw) = &self.pending_raw {
            if let Ok(Value::Object(requests)) = serde_json::from_str::<Value>(raw) {
                for r in requests.values() {
                    if model::request_survives(r, &|sid| live.contains(sid), now) {
                        pending += 1;
                        if let Some(t) = model::request_expiry(r) {
                            expiry = Some(expiry.map_or(t, |e: f64| e.min(t)));
                        }
                    }
                }
            }
        }
        model::sort_rows(&mut rows);
        let file_live = self
            .keys
            .iter()
            .filter(|k| live.contains(k.as_str()) && !overlay_live.contains(k))
            .cloned()
            .collect();
        Some(View {
            rows,
            pending,
            expiry,
            pids,
            file_live,
            overlay_live,
        })
    }
}
fn needs_repair(s: &Obj) -> bool {
    matches!(
        s.get("event").and_then(Value::as_str),
        Some("SessionStart" | "Notification")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    /// Fast path must agree with the full normalizing path on every refresh.
    #[test]
    fn matches_full_path_and_parses_only_changed_sessions() {
        let now = 1000.0;
        let mut state = json!({"version":1,"updated_at":1,"pending_requests":{
                "r1":{"session_id":"a","created_at":999.0},"r2":{"session_id":"gone","created_at":999.0}},
            "sessions":{
                "a":{"id":"a","status":"running","last_activity":990.0,"event":"PreToolUse","pending_request_id":"r1"},
                "b":{"id":"b","status":"completed","last_activity":900.0},
                "c":{"id":"c","status":"processing","event":"SessionStart","last_activity":995.0},
                "d":{"id":"d","source":"claude","status":"processing","event":"Notification","detail":"Claude is waiting for your input","last_activity":995.0},
                "e":{"id":"e","status":"running","last_activity":995.0,"terminal":{"pid":4242}}}});
        let mut alive = |p: i64| p == 4242;
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut inc = Incremental::default();
        let mut overlay = model::Obj::new();
        overlay.insert(
            "x".into(),
            json!({"id":"x","status":"running","last_activity":999.0,"provider":"app-server"}),
        );
        let view = inc.refresh(&bytes, &mut overlay, now, &mut alive).unwrap();
        assert_eq!(inc.parsed_last, 5);
        // Full path on the same input (overlay merged with or_insert).
        state["sessions"]["x"] = overlay["x"].clone();
        model::cleanup_state(&mut state, now, &mut alive);
        let (rows, pending) = model::public_sessions(&mut state, now);
        assert_eq!(view.rows, rows);
        assert_eq!(view.pending, pending);
        assert_eq!(view.expiry, model::next_expiry(&state, &mut alive));
        assert_eq!(view.pids, vec![4242]);
        // Rewrite with one session changed: only that one is re-parsed.
        let mut next: Value = serde_json::from_slice(&bytes).unwrap();
        next["updated_at"] = json!(2);
        next["sessions"]["a"]["detail"] = json!("new");
        let v2 = inc
            .refresh(
                &serde_json::to_vec(&next).unwrap(),
                &mut overlay,
                now,
                &mut alive,
            )
            .unwrap();
        assert_eq!(inc.parsed_last, 1);
        assert!(v2.rows.iter().any(|r| r.id == "a" && r.detail == "new"));
        // Removal from the file drops it from the cache.
        next["sessions"].as_object_mut().unwrap().remove("a");
        let v3 = inc
            .refresh(
                &serde_json::to_vec(&next).unwrap(),
                &mut overlay,
                now,
                &mut alive,
            )
            .unwrap();
        assert_eq!(inc.parsed_last, 0);
        assert!(!v3.rows.iter().any(|r| r.id == "a"));
        assert_eq!(v3.pending, 0, "request of a removed session is not counted");
    }
}
