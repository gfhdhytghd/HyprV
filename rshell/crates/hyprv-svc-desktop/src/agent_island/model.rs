//! Port of `quickshell/scripts/agent_island.py`. The persisted `state.json`
//! (version 1) stays the interchange with legacy Python hooks, so sessions are
//! manipulated as JSON objects and unknown fields survive read-modify-write.
use crate::util::{basename, fnv1a, Proc};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

pub const STATE_VERSION: i64 = 1;
pub const PROCESS_SCAN_INTERVAL_SECONDS: f64 = 1.5;
pub const AGENT_FOCUS_CLASS_KEYWORDS: &[&str] = &[
    "alacritty",
    "ghostty",
    "kitty",
    "wezterm",
    "foot",
    "konsole",
    "terminal",
    "terminator",
    "kgx",
    "code-oss",
    "codium",
    "visual studio code",
    "vscodium",
];
const ACTIVITY_EVENTS: &[&str] = &[
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "PreCompact",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "SessionEnd",
];

pub type Obj = Map<String, Value>;

pub fn default_state(now: f64) -> Value {
    json!({"version": STATE_VERSION, "updated_at": now, "sessions": {}, "pending_requests": {}})
}
/// `read_state` normalization: invalid roots and member types are reset.
pub fn normalize_state(data: Option<Value>, now: f64) -> Value {
    let mut data = match data {
        Some(Value::Object(o)) => Value::Object(o),
        _ => return default_state(now),
    };
    let o = data.as_object_mut().unwrap();
    o.entry("version").or_insert(json!(STATE_VERSION));
    o.entry("updated_at").or_insert(json!(0));
    for key in ["sessions", "pending_requests"] {
        if !o.get(key).is_some_and(Value::is_object) {
            o.insert(key.into(), json!({}));
        }
    }
    data
}

pub fn normalize_source(value: Option<&str>) -> Option<&'static str> {
    let v = value?.trim().to_lowercase();
    let v = match v.as_str() {
        "claude-code" | "claudecode" | "claude_code" => "claude",
        other => other,
    };
    match v {
        "codex" => Some("codex"),
        "claude" => Some("claude"),
        _ => None,
    }
}
pub fn source_label(source: &str) -> String {
    match source {
        "codex" => "Codex".into(),
        "claude" => "Claude Code".into(),
        other => title_case(other),
    }
}
fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}
/// `Path(cwd).name or cwd`, "Session" when absent.
pub fn project_name(cwd: Option<&str>) -> String {
    match cwd {
        None | Some("") => "Session".into(),
        Some(cwd) => {
            let name = path_name(cwd);
            if name.is_empty() {
                cwd.into()
            } else {
                name.into()
            }
        }
    }
}
fn path_name(p: &str) -> &str {
    let trimmed = p.trim_end_matches('/');
    let name = basename(trimmed);
    if name == "." || name == ".." {
        ""
    } else {
        name
    }
}
fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

pub fn first_string(data: &Obj, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match data.get(*k) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    })
}
pub fn event_name(payload: &Obj, event_arg: Option<&str>) -> Option<String> {
    first_string(
        payload,
        &[
            "hook_event_name",
            "hookEventName",
            "event_name",
            "eventName",
            "event",
        ],
    )
    .or_else(|| event_arg.map(str::to_string))
}
pub fn normalize_event(name: Option<&str>) -> String {
    let Some(name) = name else {
        return String::new();
    };
    match name {
        "session_start" => "SessionStart",
        "sessionEnd" | "session_end" => "SessionEnd",
        "userPromptSubmit" | "user_prompt_submit" => "UserPromptSubmit",
        "preToolUse" | "pre_tool_use" => "PreToolUse",
        "postToolUse" | "post_tool_use" => "PostToolUse",
        "post_tool_use_failure" => "PostToolUseFailure",
        "permission_request" => "PermissionRequest",
        "notification" => "Notification",
        "stop" => "Stop",
        "pre_compact" => "PreCompact",
        "subagent_start" => "SubagentStart",
        "subagent_stop" => "SubagentStop",
        other => other,
    }
    .to_string()
}
fn nested<'a>(payload: &'a Obj, keys: &[&str]) -> Option<&'a Obj> {
    keys.iter().find_map(|k| match payload.get(*k) {
        Some(Value::Object(o)) => Some(o),
        _ => None,
    })
}
pub fn tool_name(payload: &Obj) -> Option<String> {
    first_string(payload, &["tool_name", "toolName", "tool", "name"]).or_else(|| {
        nested(payload, &["tool", "payload", "data"])
            .and_then(|n| first_string(n, &["name", "tool_name", "toolName"]))
    })
}
pub fn tool_input(payload: &Obj) -> Option<&Obj> {
    // Python's `if value:` treats an empty dict as absent.
    let direct = nested(
        payload,
        &[
            "tool_input",
            "toolInput",
            "input",
            "arguments",
            "args",
            "params",
        ],
    );
    if let Some(d) = direct.filter(|d| !d.is_empty()) {
        return Some(d);
    }
    nested(payload, &["tool", "payload", "data"]).and_then(|n| {
        nested(
            n,
            &[
                "input",
                "tool_input",
                "toolInput",
                "arguments",
                "args",
                "params",
            ],
        )
    })
}
pub fn tool_use_id(payload: &Obj) -> Option<String> {
    first_string(payload, &["tool_use_id", "toolUseId"]).or_else(|| {
        nested(payload, &["tool", "tool_use", "toolUse", "payload", "data"])
            .and_then(|n| first_string(n, &["id", "tool_use_id", "toolUseId"]))
    })
}
fn str_of<'a>(o: &'a Obj, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Value::as_str)
}
pub fn tool_description(payload: &Obj) -> String {
    let name = tool_name(payload).unwrap_or_default();
    let empty = Obj::new();
    let inp = tool_input(payload).unwrap_or(&empty);
    if name == "Bash" {
        let desc = str_of(inp, "description")
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let cmd = str_of(inp, "command")
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match (desc, cmd) {
            (Some(d), Some(c)) if d != c => return format!("{d}: {}", truncate(c, 80)),
            (Some(d), _) => return d.into(),
            (None, Some(c)) => return truncate(c, 100),
            _ => {}
        }
    }
    for key in ["file_path", "path"] {
        if let Some(v) = str_of(inp, key).filter(|v| !v.trim().is_empty()) {
            let name = path_name(v);
            return if name.is_empty() {
                v.trim().into()
            } else {
                name.into()
            };
        }
    }
    for key in [
        "pattern",
        "query",
        "prompt",
        "command",
        "description",
        "message",
        "text",
        "content",
    ] {
        if let Some(v) = str_of(inp, key).filter(|v| !v.trim().is_empty()) {
            return truncate(v.trim(), 120);
        }
    }
    if let Some(v) = first_string(
        payload,
        &[
            "message", "text", "summary", "status", "detail", "content", "prompt",
        ],
    ) {
        return truncate(&v, 120);
    }
    if name.is_empty() {
        "Working".into()
    } else {
        name
    }
}
pub fn session_id(payload: &Obj, source: &str, ppid: i64) -> String {
    if let Some(id) = first_string(payload, &["session_id", "sessionId"]) {
        return id;
    }
    let cwd = first_string(payload, &["cwd"]).unwrap_or_default();
    format!("{source}-ppid-{ppid}-{}", fnv1a(cwd.as_bytes()) % 100000)
}
pub fn session_title(payload: &Obj, sid: &str) -> String {
    first_string(payload, &["session_title", "title", "name"]).unwrap_or_else(|| truncate(sid, 8))
}

pub struct Question {
    pub question: String,
    pub options: Vec<String>,
    pub answer_key: String,
}
fn option_labels(raw: Option<&Value>) -> Vec<String> {
    raw.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|o| match o {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(m) => m.get("label").and_then(Value::as_str).map(str::to_string),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}
pub fn extract_question(payload: &Obj) -> Option<Question> {
    if let Some(q) = str_of(payload, "question")
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        // Top-level options are kept verbatim when they are a list.
        let options = payload
            .get("options")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Some(Question {
            question: q.into(),
            options,
            answer_key: "answer".into(),
        });
    }
    let empty = Obj::new();
    let inp = tool_input(payload).unwrap_or(&empty);
    if let Some(first) = inp
        .get("questions")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        if let Some(first) = first.as_object() {
            if let Some(q) = str_of(first, "question")
                .map(str::trim)
                .filter(|q| !q.is_empty())
            {
                return Some(Question {
                    question: q.into(),
                    options: option_labels(first.get("options")),
                    answer_key: str_of(first, "header").unwrap_or("answer_1").into(),
                });
            }
        }
    }
    if let Some(q) = str_of(inp, "question")
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        return Some(Question {
            question: q.into(),
            options: option_labels(inp.get("options")),
            answer_key: "answer".into(),
        });
    }
    None
}
pub fn is_done_notification(source: &str, detail: &str) -> bool {
    source == "claude"
        && detail
            .trim()
            .to_lowercase()
            .contains("waiting for your input")
}
pub fn status_priority(status: &str) -> i32 {
    match status {
        "waitingApproval" => 5,
        "waitingQuestion" => 4,
        "running" => 3,
        "processing" => 2,
        "completed" => 1,
        _ => 0,
    }
}

/// Python `float(x or 0)` for JSON numbers/strings.
pub fn num(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => *b as i64 as f64,
        _ => 0.0,
    }
}
/// Python `int(value)` or None.
pub fn int_value(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}
pub fn session_pid(session: &Obj) -> Option<i64> {
    session
        .get("terminal")
        .and_then(Value::as_object)
        .and_then(|t| int_value(t.get("pid")))
}
fn sessions_mut(state: &mut Value) -> &mut Obj {
    let o = state.as_object_mut().expect("normalized state");
    if !o.get("sessions").is_some_and(Value::is_object) {
        o.insert("sessions".into(), json!({}));
    }
    o.get_mut("sessions")
        .and_then(Value::as_object_mut)
        .unwrap()
}
fn pending_mut(state: &mut Value) -> &mut Obj {
    let o = state.as_object_mut().expect("normalized state");
    if !o.get("pending_requests").is_some_and(Value::is_object) {
        o.insert("pending_requests".into(), json!({}));
    }
    o.get_mut("pending_requests")
        .and_then(Value::as_object_mut)
        .unwrap()
}

/// Facts the hook process gathers about its terminal (`terminal_meta`).
pub fn terminal_meta(env: &HashMap<String, String>, window: Option<Value>, pid: i64) -> Value {
    let mut meta = Obj::new();
    for (env_key, out_key) in [
        ("TERM_PROGRAM", "term_app"),
        ("TERM_PROGRAM_VERSION", "term_version"),
        ("__CFBundleIdentifier", "term_bundle"),
        ("KITTY_WINDOW_ID", "kitty_window"),
        ("KITTY_LISTEN_ON", "kitty_listen_on"),
        ("TMUX_PANE", "tmux_pane"),
        ("TMUX", "tmux"),
    ] {
        if let Some(v) = env.get(env_key).filter(|v| !v.is_empty()) {
            meta.insert(out_key.into(), json!(v));
        }
    }
    if let Some(w) = window {
        let has_address = w
            .get("address")
            .and_then(Value::as_str)
            .is_some_and(|a| !a.is_empty());
        if has_address && is_agent_focus_client(&w) {
            meta.insert("window".into(), w);
        }
    }
    meta.insert("pid".into(), json!(pid));
    Value::Object(meta)
}
/// `active_window_snapshot` projection of `hyprctl activewindow -j`.
pub fn window_snapshot(data: &Value) -> Option<Value> {
    let o = data.as_object()?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
    Some(json!({
        "address": s("address").unwrap_or(""),
        "class": s("class").or(s("initialClass")).unwrap_or(""),
        "initialClass": s("initialClass").unwrap_or(""),
        "title": s("title").or(s("initialTitle")).unwrap_or(""),
        "pid": o.get("pid").filter(|v| truthy(Some(v))).cloned().unwrap_or(json!(0)),
    }))
}
pub fn is_agent_focus_client(client: &Value) -> bool {
    let Some(c) = client.as_object() else {
        return false;
    };
    let class_text = format!(
        "{} {}",
        c.get("class").and_then(Value::as_str).unwrap_or(""),
        c.get("initialClass").and_then(Value::as_str).unwrap_or("")
    )
    .to_lowercase();
    AGENT_FOCUS_CLASS_KEYWORDS
        .iter()
        .any(|k| class_text.contains(k))
}

/// `update_session_from_event`. Returns the pending request id, if one was created.
pub fn update_session_from_event(
    state: &mut Value,
    payload: &Obj,
    source: &str,
    event: &str,
    terminal: Value,
    ppid: i64,
    now: f64,
) -> (String, Option<String>) {
    let sid = session_id(payload, source, ppid);
    let sessions = sessions_mut(state);
    let mut session = match sessions.get(&sid) {
        Some(Value::Object(o)) => o.clone(),
        _ => Obj::new(),
    };
    session.entry("id").or_insert(json!(sid));
    session.insert("source".into(), json!(source));
    session.insert("source_label".into(), json!(source_label(source)));
    session.insert("title".into(), json!(session_title(payload, &sid)));
    let cwd = first_string(payload, &["cwd"])
        .or_else(|| {
            str_of(&session, "cwd")
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default();
    session.insert("project".into(), json!(project_name(Some(&cwd))));
    session.insert("cwd".into(), json!(cwd));
    if let Some(model) = first_string(payload, &["model"]) {
        session.insert("model".into(), json!(model));
    }
    session.insert("event".into(), json!(event));
    session.insert("last_activity".into(), json!(now));
    session.entry("started_at").or_insert(json!(now));
    if truthy(Some(&terminal)) {
        session.insert("terminal".into(), terminal);
    }
    if ACTIVITY_EVENTS.contains(&event) {
        session.insert("had_activity".into(), json!(true));
    } else if event == "SessionStart" {
        session.entry("had_activity").or_insert(json!(false));
    }

    let mut request: Option<Value> = None;
    let detail = tool_description(payload);
    let name = tool_name(payload);
    let set = |s: &mut Obj, k: &str, v: Value| {
        s.insert(k.into(), v);
    };
    match event {
        "SessionStart" => {
            set(&mut session, "status", json!("idle"));
            set(&mut session, "detail", json!("Session started"));
            set(&mut session, "progress", json!(0.0));
        }
        "UserPromptSubmit" => {
            set(&mut session, "status", json!("processing"));
            set(&mut session, "current_tool", json!(""));
            let d = first_string(payload, &["prompt", "user_prompt", "message", "input"])
                .unwrap_or_else(|| "Reading prompt".into());
            set(&mut session, "detail", json!(d));
            set(&mut session, "progress", json!(0.16));
        }
        "PreToolUse" | "SubagentStart" => {
            set(&mut session, "status", json!("running"));
            let tool = name.clone().unwrap_or_else(|| {
                if event == "SubagentStart" {
                    "Agent"
                } else {
                    "Tool"
                }
                .into()
            });
            set(&mut session, "current_tool", json!(tool));
            set(&mut session, "detail", json!(detail));
            set(&mut session, "progress", json!(0.42));
        }
        "PostToolUse" | "PostToolUseFailure" | "SubagentStop" => {
            let history = session.entry("tool_history").or_insert(json!([]));
            if let (Some(list), Some(n)) = (history.as_array_mut(), name.as_ref()) {
                list.push(json!({
                    "tool": n, "detail": detail,
                    "success": event != "PostToolUseFailure", "time": now,
                }));
                let excess = list.len().saturating_sub(8);
                list.drain(..excess);
            }
            set(&mut session, "status", json!("processing"));
            set(&mut session, "current_tool", json!(""));
            let n = name.as_deref().unwrap_or("tool");
            set(&mut session, "detail", json!(format!("Finished {n}")));
            set(&mut session, "progress", json!(0.66));
        }
        "PermissionRequest" => {
            let question = (name.as_deref() == Some("AskUserQuestion"))
                .then(|| extract_question(payload))
                .flatten();
            let suffix =
                tool_use_id(payload).unwrap_or_else(|| ((now * 1000.0) as i64).to_string());
            let rid = format!("{source}-{sid}-{suffix}");
            if let Some(q) = question {
                set(&mut session, "status", json!("waitingQuestion"));
                set(&mut session, "current_tool", json!("AskUserQuestion"));
                set(&mut session, "detail", json!(q.question));
                request = Some(json!({
                    "id": rid, "kind": "question_from_permission", "session_id": sid,
                    "source": source, "question": q.question, "options": q.options,
                    "answer_key": q.answer_key, "created_at": now,
                }));
            } else {
                let tool = name.clone().unwrap_or_else(|| "Permission".into());
                set(&mut session, "status", json!("waitingApproval"));
                set(&mut session, "current_tool", json!(tool));
                set(&mut session, "detail", json!(detail));
                request = Some(json!({
                    "id": rid, "kind": "approval", "session_id": sid, "source": source,
                    "tool": tool, "detail": detail, "created_at": now,
                }));
            }
            set(&mut session, "pending_request_id", json!(rid));
            set(&mut session, "progress", json!(0.82));
        }
        "Notification" => {
            if let Some(q) = extract_question(payload) {
                let rid = format!("{source}-{sid}-{}", (now * 1000.0) as i64);
                set(&mut session, "status", json!("waitingQuestion"));
                set(&mut session, "current_tool", json!("Question"));
                set(&mut session, "detail", json!(q.question));
                request = Some(json!({
                    "id": rid, "kind": "question", "session_id": sid, "source": source,
                    "question": q.question, "options": q.options,
                    "answer_key": q.answer_key, "created_at": now,
                }));
                set(&mut session, "pending_request_id", json!(rid));
            } else {
                set(&mut session, "detail", json!(detail));
                if is_done_notification(source, &detail) {
                    set(&mut session, "status", json!("completed"));
                    set(&mut session, "current_tool", json!(""));
                    set(&mut session, "progress", json!(1.0));
                    set(&mut session, "completed_at", json!(now));
                    session.remove("pending_request_id");
                } else if matches!(session.get("status"), None | Some(Value::Null))
                    || str_of(&session, "status") == Some("idle")
                {
                    set(&mut session, "status", json!("processing"));
                }
                let current = match session.get("progress") {
                    Some(v) if truthy(Some(v)) => num(Some(v)),
                    _ => 0.5,
                };
                set(&mut session, "progress", json!(current.max(0.5)));
            }
        }
        "PreCompact" => {
            set(&mut session, "status", json!("processing"));
            set(&mut session, "current_tool", json!(""));
            set(&mut session, "detail", json!("Compacting context"));
            set(&mut session, "progress", json!(0.55));
        }
        "Stop" | "SessionEnd" => {
            set(&mut session, "status", json!("completed"));
            set(&mut session, "current_tool", json!(""));
            let d = if event == "Stop" {
                first_string(
                    payload,
                    &["last_assistant_message", "text", "message", "summary"],
                )
                .unwrap_or_else(|| "Finished".into())
            } else {
                "Session ended".into()
            };
            set(&mut session, "detail", json!(d));
            set(&mut session, "progress", json!(1.0));
            set(&mut session, "completed_at", json!(now));
            session.remove("pending_request_id");
        }
        _ => {
            let status = session
                .get("status")
                .filter(|v| truthy(Some(v)))
                .cloned()
                .unwrap_or(json!("processing"));
            set(&mut session, "status", status);
            set(&mut session, "detail", json!(detail));
        }
    }
    sessions.insert(sid.clone(), Value::Object(session));
    let rid = request
        .as_ref()
        .map(|r| r["id"].as_str().unwrap_or_default().to_string());
    if let Some(r) = request {
        pending_mut(state).insert(rid.clone().unwrap(), r);
    }
    (sid, rid)
}

/// The hook is observation-only: drop the request it just recorded.
pub fn drop_request(state: &mut Value, sid: &str, rid: &str) {
    pending_mut(state).remove(rid);
    if let Some(Value::Object(s)) = sessions_mut(state).get_mut(sid) {
        s.remove("pending_request_id");
    }
}

/// `cleanup_state`. `alive` answers PID liveness.
/// Per-session half of `cleanup_state`: does this session survive at `now`?
pub fn session_survives(session: &Obj, now: f64, alive: &mut dyn FnMut(i64) -> bool) -> bool {
    let status = str_of(session, "status").unwrap_or("");
    let pid = session_pid(session);
    let has_pending = truthy(session.get("pending_request_id"));
    if let Some(pid) = pid {
        if !alive(pid) && !has_pending {
            return false;
        }
    }
    if held_by_process(session, status) && pid.is_some_and(&mut *alive) {
        return true;
    }
    now - session_reference_time(session) <= stale_after(status)
}
/// Pending-request half of `cleanup_state`.
pub fn request_survives(request: &Value, live: &dyn Fn(&str) -> bool, now: f64) -> bool {
    match request {
        Value::Object(r) => {
            let sid = r.get("session_id").and_then(Value::as_str).unwrap_or("");
            live(sid) && now - num(r.get("created_at")) <= 86400.0
        }
        _ => false,
    }
}
/// `cleanup_state`. `alive` answers PID liveness.
pub fn cleanup_state(state: &mut Value, now: f64, alive: &mut dyn FnMut(i64) -> bool) {
    let sessions = sessions_mut(state);
    sessions.retain(|_, s| {
        s.as_object()
            .is_some_and(|o| session_survives(o, now, alive))
    });
    let live: HashSet<String> = sessions.keys().cloned().collect();
    pending_mut(state).retain(|_, r| request_survives(r, &|sid| live.contains(sid), now));
}
/// A completed turn never goes stale while its process lives.
fn held_by_process(session: &Obj, status: &str) -> bool {
    status == "completed" && truthy(session.get("had_activity"))
}
fn session_reference_time(session: &Obj) -> f64 {
    let last = num(session.get("last_activity"));
    let completed = match session.get("completed_at") {
        Some(v) if truthy(Some(v)) => num(Some(v)),
        _ => last,
    };
    last.max(completed)
}
fn stale_after(status: &str) -> f64 {
    if matches!(status, "completed" | "idle") {
        20.0
    } else {
        900.0
    }
}
/// When this session would expire, unless it is kept alive by its process.
pub fn session_expiry(session: &Obj, alive: &mut dyn FnMut(i64) -> bool) -> Option<f64> {
    let status = str_of(session, "status").unwrap_or("");
    let pid = session_pid(session);
    if held_by_process(session, status) && pid.is_some_and(&mut *alive) {
        return None;
    }
    Some(session_reference_time(session) + stale_after(status))
}
pub fn request_expiry(request: &Value) -> Option<f64> {
    request
        .as_object()
        .map(|r| num(r.get("created_at")) + 86400.0)
}
/// Earliest epoch at which `cleanup_state` would change anything time-based.
pub fn next_expiry(state: &Value, alive: &mut dyn FnMut(i64) -> bool) -> Option<f64> {
    let sessions = state.get("sessions").and_then(Value::as_object);
    let pending = state.get("pending_requests").and_then(Value::as_object);
    let a = sessions
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(Value::as_object)
        .filter_map(|s| session_expiry(s, alive));
    let b = pending
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(request_expiry);
    a.chain(b).reduce(f64::min)
}

/// One public session row (`desktop-state.py` display fields).
#[derive(Clone, Debug, PartialEq)]
pub struct PublicSession {
    pub id: String,
    pub source: String,
    pub source_label: String,
    pub title: String,
    pub project: String,
    pub status: String,
    pub current_tool: String,
    pub detail: String,
    pub progress: f64,
    pub last_activity: f64,
    /// A running ProcessScan row: shown at `progress`, not the 12 s phase.
    pub pinned: bool,
}
/// `public_snapshot`'s in-place repairs (SessionStart idle, done notification).
pub fn repair_session(session: &mut Obj, now: f64) {
    if str_of(session, "event") == Some("SessionStart")
        && !truthy(session.get("pending_request_id"))
    {
        session.insert("status".into(), json!("idle"));
        session.insert("detail".into(), json!("Session started"));
    }
    if str_of(session, "event") == Some("Notification")
        && is_done_notification(
            str_of(session, "source").unwrap_or(""),
            str_of(session, "detail").unwrap_or(""),
        )
    {
        session.insert("status".into(), json!("completed"));
        session.insert("current_tool".into(), json!(""));
        session.insert("progress".into(), json!(1.0));
        session.entry("completed_at").or_insert(json!(now));
    }
}
/// Display row of a repaired session; `None` for idle (hidden) sessions.
pub fn public_row(session: &Obj) -> Option<PublicSession> {
    let status = str_of(session, "status")
        .filter(|s| !s.is_empty())
        .unwrap_or("idle");
    if status == "idle" {
        return None;
    }
    let text = |k: &str| match session.get(k) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    };
    Some(PublicSession {
        id: text("id"),
        source: text("source"),
        source_label: text("source_label"),
        title: text("title"),
        project: text("project"),
        status: status.to_string(),
        current_tool: text("current_tool"),
        detail: text("detail"),
        progress: num(session.get("progress")),
        last_activity: num(session.get("last_activity")),
        // Legacy re-anchored `last_activity` every 1.5 s scan, so the phase
        // never rose above ~0.35 and these rows sat at their 0.38/0.46 floor.
        // The actor re-applies the scan time only on refresh; pin them.
        pinned: status == "running" && str_of(session, "event") == Some("ProcessScan"),
    })
}
/// Priority, then most recent activity first.
pub fn sort_rows(rows: &mut [PublicSession]) {
    sort_rows_by(rows, &|_| 0);
}
/// Python's stable `sort(key=(priority, last_activity), reverse=True)`: equal
/// keys keep dict order, which `rank` gives (see [`LegacyOrder`]).
pub fn sort_rows_by(rows: &mut [PublicSession], rank: &dyn Fn(&str) -> u64) {
    rows.sort_by(|a, b| {
        (status_priority(&b.status), b.last_activity)
            .partial_cmp(&(status_priority(&a.status), a.last_activity))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| rank(&a.id).cmp(&rank(&b.id)))
            // Deterministic ties, so an unchanged state never reorders.
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Legacy dict order of `state["sessions"]`: insertion order, where a removed
/// and re-added session goes to the end. Hooks and the legacy observer both
/// rewrote state.json preserving it; the read-only actor reproduces it from
/// the file order of hook sessions and the creation order of its overlay.
#[derive(Default)]
pub struct LegacyOrder {
    seq: HashMap<String, u64>,
    next: u64,
}
impl LegacyOrder {
    /// `file`: live file sessions in file order; `overlay`: live sessions only
    /// in the actor's overlay (created by the scan in codex-then-claude, PID
    /// order, or by app-server messages).
    pub fn update(&mut self, file: &[String], overlay: &[String]) {
        let live: HashSet<&str> = file.iter().chain(overlay).map(String::as_str).collect();
        self.seq.retain(|id, _| live.contains(id.as_str()));
        let mut fresh: Vec<&String> = overlay
            .iter()
            .filter(|id| !self.seq.contains_key(*id))
            .collect();
        fresh.sort_by_key(|id| creation_key(id));
        for id in file.iter().chain(fresh) {
            if !self.seq.contains_key(id) {
                self.seq.insert(id.clone(), self.next);
                self.next += 1;
            }
        }
    }
    pub fn rank(&self, id: &str) -> u64 {
        self.seq.get(id).copied().unwrap_or(u64::MAX)
    }
}
/// The order `sync_process_sessions` inserts new synthetic sessions.
fn creation_key(id: &str) -> (u8, i64, &str) {
    let pid = |rest: &str| {
        rest.split('-')
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0)
    };
    match (
        id.strip_prefix("codexproc-"),
        id.strip_prefix("claudeproc-"),
    ) {
        (Some(rest), _) => (0, pid(rest), id),
        (_, Some(rest)) => (1, pid(rest), id),
        _ => (2, 0, id),
    }
}
/// Keys of the top-level `sessions` object in document order.
pub fn session_keys(bytes: &[u8]) -> Vec<String> {
    use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
    struct Keys;
    impl<'de> Visitor<'de> for Keys {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an object")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(k) = map.next_key::<std::borrow::Cow<str>>()? {
                map.next_value::<IgnoredAny>()?;
                out.push(k.into_owned());
            }
            Ok(out)
        }
    }
    struct Top;
    impl<'de> Visitor<'de> for Top {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an object")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(k) = map.next_key::<std::borrow::Cow<str>>()? {
                if k == "sessions" {
                    out = map.next_value_seed(Seed)?;
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(out)
        }
    }
    struct Seed;
    impl<'de> serde::de::DeserializeSeed<'de> for Seed {
        type Value = Vec<String>;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Vec<String>, D::Error> {
            d.deserialize_any(KeysOrNone)
        }
    }
    struct KeysOrNone;
    impl<'de> Visitor<'de> for KeysOrNone {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("sessions")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Vec<String>, A::Error> {
            Keys.visit_map(map)
        }
        fn visit_unit<E>(self) -> Result<Vec<String>, E> {
            Ok(Vec::new())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Vec<String>, A::Error> {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(Vec::new())
        }
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    de.deserialize_map(Top).unwrap_or_default()
}

/// The legacy observer's process-scan clock (`last_process_scan`): a scan at
/// most every [`PROCESS_SCAN_INTERVAL_SECONDS`]. Discovered sessions carry the
/// time of the last scan as `last_activity`; the actor re-applies it only
/// when the legacy view would observably change.
#[derive(Default)]
pub struct ScanClock {
    pub at: Option<f64>,
    resync_after: Option<f64>,
}
impl ScanClock {
    /// Whether a legacy tick at `now` scans.
    pub fn due(&self, now: f64) -> bool {
        self.at
            .is_none_or(|t| now - t >= PROCESS_SCAN_INTERVAL_SECONDS)
    }
    /// A scan at `now`; true when the view must be recomputed: the discovered
    /// set changed, or a session a hook touched since the previous scan is now
    /// re-synced/reordered by this one.
    pub fn scanned(&mut self, now: f64, discovery_changed: bool) -> bool {
        self.at = Some(now);
        discovery_changed || self.resync_after.is_some_and(|l| l <= now)
    }
    /// After a refresh: the earliest `last_activity` newer than the scan.
    pub fn refreshed(&mut self, resync_after: Option<f64>) {
        self.resync_after = resync_after;
    }
}
/// Earliest `last_activity` at or after scan time `scan` of a session that scan
/// did not stamp (see [`ScanClock`]): the next scan re-stamps the discovered
/// sessions past it, which re-sorts (ties keep dict order) or re-syncs it.
pub fn resync_after(state: &Value, scan: f64) -> Option<f64> {
    state
        .get("sessions")
        .and_then(Value::as_object)?
        .values()
        .filter(|s| num(s.get("process_scan_seen_at")) != scan)
        .map(|s| num(s.get("last_activity")))
        .filter(|l| *l >= scan)
        .reduce(f64::min)
}
/// `public_snapshot` without the time-dependent progress animation; mutates
/// `state` like the source (SessionStart/idle and done-notification repair).
pub fn public_sessions(state: &mut Value, now: f64) -> (Vec<PublicSession>, usize) {
    let mut out = Vec::new();
    for session in sessions_mut(state)
        .values_mut()
        .filter_map(Value::as_object_mut)
    {
        repair_session(session, now);
        out.extend(public_row(session));
    }
    sort_rows(&mut out);
    let pending = state
        .get("pending_requests")
        .and_then(Value::as_object)
        .map(|p| p.values().filter(|v| v.is_object()).count())
        .unwrap_or(0);
    (out, pending)
}
/// Synthetic progress for running/processing rows (12 s phase, cap 0.94).
pub fn display_progress(status: &str, base: f64, last_activity: f64, now: f64) -> f64 {
    if matches!(status, "running" | "processing") {
        let anchor = if last_activity != 0.0 {
            last_activity
        } else {
            now
        };
        let phase = (now - anchor).rem_euclid(12.0) / 12.0;
        (0.22 + phase * 0.66).max(base).min(0.94)
    } else {
        base
    }
}

// ------------------------------------------------------------ process scan

pub struct ProcInfo {
    pub ppid: i64,
    pub start_ticks: u64,
    pub cmdline: Vec<String>,
}
/// Full readdir + re-stat of every PID (catches a late `exec` in an old PID,
/// reparenting, PID reuse and threads picked up by the range probe).
pub const PROCESS_SWEEP_SECONDS: f64 = 60.0;
/// More new IDs than this since the last probe: a readdir is cheaper.
const MAX_RANGE: i64 = 512;
/// At most one exec-triggered full re-stat per this many seconds. Unprivileged
/// fanotify reports no PID, and Codex execs short-lived helpers of its own
/// binary several times a minute, so every event cannot cost a full pass.
pub const EXEC_RESCAN_SECONDS: f64 = 15.0;
/// Ancestry walks stop after this many links (cycles, absurd depths).
const MAX_CHAIN: usize = 64;
const ARGV_FOR: &[&str] = &["codex", "claude", "systemd-inhibit"];
struct Seen {
    comm: String,
    stat: hyprv_core::sample_file::SampleFile,
    cmdline: Option<hyprv_core::sample_file::SampleFile>,
    codex: bool,
    claude: bool,
    inhibitor: bool,
}
/// Incremental `list_processes`: the same PID → (ppid, start, argv) map.
///
/// A probe (every 1.5 s) costs, independent of how many processes exist:
/// - one poll(2) over the pidfds of agent-named PIDs (exits);
/// - stats of the IDs allocated since the previous probe (`/proc/loadavg`);
/// - one more stat of every PID first seen by the previous refresh, including
///   the initial snapshot: fork-then-exec happens right after the PID appears
///   (DESKTOP-R2-2);
/// - stats of agent-named PIDs (exec away from an agent, argv changes) and of
///   every link of each inhibitor's ancestry chain, so a reused or reparented
///   ancestor is never trusted from the cache (DESKTOP-R2-1);
/// - one more stat of every PID that any stat of the previous refresh saw
///   running or in disk sleep (`R`/`D`): it may have been inside `execve` with
///   its old comm and argv still visible.
///
/// A late `exec` of an agent binary in an old, non-agent PID is signalled by
/// fanotify `FAN_OPEN_EXEC` on the agent executables ([`ExecWatch`]) and answered
/// by one full re-stat, at most every [`EXEC_RESCAN_SECONDS`]; a pending signal
/// waits out the limit. The event is queued when `execve` opens the binary,
/// before the new image is visible, so the pass may still read the old one; the
/// `R`/`D` re-stat above covers that (an exec blocked in an interruptible
/// filesystem wait, e.g. FUSE, is left to the sweep). Every
/// [`PROCESS_SWEEP_SECONDS`] a full readdir reconciles the map, which also covers
/// agents started from unmarked paths.
#[derive(Default)]
pub struct ProcTable {
    pub procs: HashMap<i64, ProcInfo>,
    seen: HashMap<i64, Seen>,
    exits: HashMap<i64, std::os::fd::OwnedFd>,
    last_pid: Option<i32>,
    last_sweep: Option<f64>,
    key: Vec<(i64, u64, &'static str)>,
    next_key: Vec<(i64, u64, &'static str)>,
    recheck: Vec<i64>,
    polls: Vec<libc::pollfd>,
    poll_pids: Vec<i64>,
    codex: HashSet<i64>,
    active: HashSet<i64>,
    directory: Option<hyprv_core::sample_file::PidDirectory>,
    root: std::path::PathBuf,
    fd_budget: usize,
    allocation_files: Option<(
        hyprv_core::sample_file::SampleFile,
        hyprv_core::sample_file::SampleFile,
    )>,
    /// PIDs first seen by the previous refresh / by this one.
    fresh: Vec<i64>,
    fresh_next: Vec<i64>,
    /// PIDs a stat of the previous refresh / of this one saw in `R` or `D`.
    busy: Vec<i64>,
    busy_next: Vec<i64>,
    /// Scratch: inhibitors and chain links validated in this probe.
    inhibitors: Vec<i64>,
    validated: Vec<i64>,
    exec: Option<crate::util::ExecWatch>,
    exec_tried: bool,
    exec_pending: bool,
    last_exec_scan: Option<f64>,
}
impl ProcTable {
    pub fn sweep_due(&self, now: f64) -> bool {
        self.last_sweep
            .is_none_or(|t| now < t || now - t >= PROCESS_SWEEP_SECONDS)
    }
    /// Discovered agents as (pid, start ticks, source), sorted.
    pub fn agents(&self) -> &[(i64, u64, &'static str)] {
        &self.key
    }
    /// An agent binary was executed somewhere (what [`ExecWatch`] reports).
    pub fn note_exec(&mut self) {
        self.exec_pending = true;
    }
    /// Also watch `path` for execs (tests; agents launched from elsewhere are
    /// marked when discovered).
    pub fn watch_exec(&mut self, path: &std::path::Path) -> bool {
        self.exec_watch().is_some_and(|w| w.mark(path))
    }
    fn exec_watch(&mut self) -> Option<&mut crate::util::ExecWatch> {
        if !self.exec_tried {
            self.exec_tried = true;
            self.exec = crate::util::ExecWatch::new().ok();
        }
        self.exec.as_mut()
    }
    /// Marks the agent executables found on `PATH` and those of discovered
    /// agent-named processes. Re-run at every full sweep: an updated binary is
    /// a new inode.
    fn mark_agent_binaries(&mut self, proc: &Proc) {
        let exes: Vec<std::path::PathBuf> = self
            .seen
            .iter()
            .filter(|(_, s)| ARGV_FOR.contains(&s.comm.as_str()))
            .filter_map(|(pid, _)| proc.exe(*pid as i32))
            .collect();
        let Some(watch) = self.exec_watch() else {
            return;
        };
        watch.remark();
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            for name in ARGV_FOR {
                let path = dir.join(name);
                if path.is_file() {
                    watch.mark(&path);
                }
            }
        }
        for exe in exes {
            watch.mark(&exe);
        }
    }
    /// Re-reads `/proc`; true when the discovered agent set changed.
    pub fn refresh(&mut self, proc: &Proc, now: f64) -> bool {
        if self.root != proc.root {
            self.procs.clear();
            self.seen.clear();
            self.exits.clear();
            self.last_pid = None;
            self.last_sweep = None;
            self.directory = None;
            self.allocation_files = None;
            self.root.clone_from(&proc.root);
            let mut limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            self.fd_budget = if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } == 0 {
                (limit.rlim_cur as usize / 8).clamp(1, 2048)
            } else {
                128
            };
        }
        let fixture = proc.root != std::path::Path::new("/proc");
        let files = self.allocation_files.get_or_insert_with(|| {
            (
                hyprv_core::sample_file::SampleFile::new(proc.root.join("loadavg"), 1024)
                    .verify_unlinked(fixture),
                hyprv_core::sample_file::SampleFile::new(proc.root.join("sys/kernel/pid_max"), 64)
                    .verify_unlinked(fixture),
            )
        });
        let last_pid = files
            .0
            .prefix()
            .ok()
            .and_then(|s| s.split_ascii_whitespace().nth(4)?.parse().ok());
        let pid_max = files
            .1
            .prefix()
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(4_194_304);
        let range = match (self.last_pid, last_pid) {
            _ if self.sweep_due(now) => None,
            (Some(a), Some(b)) => new_ids(a, b, pid_max),
            _ => None,
        };
        self.last_pid = last_pid;
        match range {
            Some(ids) => self.probe(proc, ids, now),
            None => self.sweep(proc, now),
        }
        std::mem::swap(&mut self.fresh, &mut self.fresh_next);
        self.fresh_next.clear();
        std::mem::swap(&mut self.busy, &mut self.busy_next);
        self.busy_next.clear();
        self.next_key.clear();
        self.codex.clear();
        self.active.clear();
        for (pid, seen) in &self.seen {
            if seen.codex {
                self.codex.insert(*pid);
            }
            if seen.claude {
                if let Some(p) = self.procs.get(pid) {
                    self.next_key.push((*pid, p.start_ticks, "claude"));
                }
            }
        }
        for (pid, seen) in &self.seen {
            if !seen.inhibitor {
                continue;
            }
            let mut current = self.procs.get(pid).map_or(0, |p| p.ppid);
            for _ in 0..self.procs.len() {
                if current <= 0 {
                    break;
                }
                if self.codex.contains(&current) {
                    self.active.insert(current);
                    break;
                }
                current = self.procs.get(&current).map_or(0, |p| p.ppid);
            }
        }
        for pid in &self.active {
            if let Some(p) = self.procs.get(pid) {
                self.next_key.push((*pid, p.start_ticks, "codex"));
            }
        }
        self.next_key.sort_unstable();
        let changed = self.next_key != self.key;
        std::mem::swap(&mut self.next_key, &mut self.key);
        changed
    }
    fn sweep(&mut self, proc: &Proc, now: f64) {
        let full = self.sweep_due(now);
        if full {
            self.last_sweep = Some(now);
        }
        let mut pids = std::mem::take(&mut self.recheck);
        pids.clear();
        if self.directory.as_ref().is_some_and(|d| d.unlinked()) {
            self.directory = None;
        }
        if self.directory.is_none() {
            self.directory = hyprv_core::sample_file::PidDirectory::new(&proc.root).ok();
        }
        if self
            .directory
            .as_mut()
            .is_none_or(|d| d.pids(&mut pids).is_err())
        {
            self.directory = None;
            pids.clear();
        }
        pids.sort_unstable();
        self.procs.retain(|pid, _| pids.binary_search(pid).is_ok());
        self.seen.retain(|pid, _| pids.binary_search(pid).is_ok());
        self.exits.retain(|pid, _| pids.binary_search(pid).is_ok());
        for pid in pids.iter().copied() {
            self.restat(proc, pid, false);
        }
        self.recheck = pids;
        if full && proc.root == std::path::Path::new("/proc") {
            self.mark_agent_binaries(proc);
        }
    }
    fn probe(&mut self, proc: &Proc, ids: impl Iterator<Item = i64>, now: f64) {
        use std::os::fd::AsRawFd;
        self.polls.clear();
        self.poll_pids.clear();
        for (pid, fd) in &self.exits {
            self.poll_pids.push(*pid);
            self.polls.push(libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
        if !self.polls.is_empty()
            && unsafe { libc::poll(self.polls.as_mut_ptr(), self.polls.len() as _, 0) } > 0
        {
            for i in 0..self.polls.len() {
                if self.polls[i].revents != 0 {
                    self.forget(self.poll_pids[i]);
                }
            }
        }
        if self.exec.as_ref().is_some_and(|w| w.drain()) {
            self.exec_pending = true;
        }
        let full = self.exec_pending
            && self
                .last_exec_scan
                .is_none_or(|t| now < t || now - t >= EXEC_RESCAN_SECONDS);
        let mut recheck = std::mem::take(&mut self.recheck);
        recheck.clear();
        if full {
            // An agent binary was exec'd, maybe in an old PID: one full pass.
            // Kernel threads never exec; a reuse of their PID is a new
            // allocation, which the ID range below re-stats anyway.
            self.exec_pending = false;
            self.last_exec_scan = Some(now);
            let procs = &self.procs;
            recheck.extend(self.seen.keys().copied().filter(|p| !is_kthread(procs, *p)));
        } else {
            let procs = &self.procs;
            recheck.extend(
                self.fresh
                    .iter()
                    .copied()
                    .filter(|p| !is_kthread(procs, *p)),
            );
            recheck.extend(self.busy.iter().copied());
            recheck.extend(
                self.seen
                    .iter()
                    .filter(|(_, s)| ARGV_FOR.contains(&s.comm.as_str()))
                    .map(|(pid, _)| *pid),
            );
        }
        for pid in recheck.iter().copied().chain(ids) {
            self.restat(proc, pid, true);
        }
        self.recheck = recheck;
        if !full {
            self.validate_chains(proc);
        }
    }
    /// Re-stats every link from each inhibitor up to its Codex ancestor (or the
    /// end of the chain), so attribution never follows a cached ppid of a PID
    /// that was reused or reparented (DESKTOP-R2-1).
    fn validate_chains(&mut self, proc: &Proc) {
        let mut inhibitors = std::mem::take(&mut self.inhibitors);
        let mut validated = std::mem::take(&mut self.validated);
        inhibitors.clear();
        validated.clear();
        inhibitors.extend(
            self.seen
                .iter()
                .filter(|(_, s)| s.inhibitor)
                .map(|(p, _)| *p),
        );
        for &inhibitor in &inhibitors {
            let mut current = self.procs.get(&inhibitor).map_or(0, |p| p.ppid);
            for _ in 0..MAX_CHAIN {
                if current <= 0 {
                    break;
                }
                if !validated.contains(&current) {
                    validated.push(current);
                    self.restat(proc, current, false);
                }
                if self.seen.get(&current).is_none_or(|s| s.codex) {
                    break;
                }
                current = self.procs.get(&current).map_or(0, |p| p.ppid);
            }
        }
        self.inhibitors = inhibitors;
        self.validated = validated;
    }

    fn forget(&mut self, pid: i64) {
        self.procs.remove(&pid);
        self.seen.remove(&pid);
        self.exits.remove(&pid);
    }
    /// Stat one PID. `maybe_thread`: it came from the ID range, not a readdir,
    /// so an agent-named entry must be a thread-group leader.
    fn restat(&mut self, proc: &Proc, pid: i64, maybe_thread: bool) {
        use hyprv_core::sample_file::SampleFile;
        let fixture = proc.root != std::path::Path::new("/proc");
        let cache = self.seen.len() < self.fd_budget;
        let known = self.seen.contains_key(&pid);
        let mut seen = self.seen.remove(&pid).unwrap_or_else(|| Seen {
            comm: String::new(),
            stat: SampleFile::new(proc.root.join(format!("{pid}/stat")), 512)
                .verify_unlinked(fixture)
                .keep_open(cache),
            cmdline: None,
            codex: false,
            claude: false,
            inhibitor: false,
        });
        let Some((comm, ppid, start, busy)) = seen.stat.prefix().ok().and_then(|t| {
            let (c, p, s) = crate::util::stat_fields(t)?;
            Some((c, p, s, crate::util::stat_busy(t)))
        }) else {
            self.forget(pid);
            return;
        };
        // Maybe inside execve: look again at the next probe. Kernel threads
        // never exec.
        if busy && pid != 2 && ppid != 2 {
            self.busy_next.push(pid);
        }
        let same = self.procs.get(&pid).is_some_and(|p| p.start_ticks == start);
        let named = ARGV_FOR.contains(&comm);
        let identity_changed = !same || seen.comm != comm;
        if seen.comm != comm {
            seen.comm.clear();
            seen.comm.push_str(comm);
        }
        if named && maybe_thread && identity_changed && proc.tgid(pid as i32) != Some(pid as i32) {
            self.forget(pid);
            return;
        }
        let mut argv = self
            .procs
            .get_mut(&pid)
            .map(|p| std::mem::take(&mut p.cmdline))
            .unwrap_or_default();
        let argv_changed = if named {
            let f = seen.cmdline.get_or_insert_with(|| {
                SampleFile::growable(
                    proc.root.join(format!("{pid}/cmdline")),
                    1024,
                    8 * 1024 * 1024,
                )
                .verify_unlinked(fixture)
                .keep_open(cache)
            });
            let raw = f.bytes().unwrap_or(&[]);
            let mut fields = raw.split(|b| *b == 0).filter(|s| !s.is_empty());
            let equal = argv
                .iter()
                .all(|cached| fields.next().is_some_and(|bytes| lossy_eq(cached, bytes)))
                && fields.next().is_none();
            if !equal {
                let mut count = 0;
                for text in raw
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                    .map(String::from_utf8_lossy)
                {
                    if let Some(slot) = argv.get_mut(count) {
                        slot.clear();
                        slot.push_str(&text);
                    } else {
                        argv.push(text.into_owned());
                    }
                    count += 1;
                }
                argv.truncate(count);
            }
            !equal
        } else {
            let changed = !argv.is_empty();
            argv.clear();
            seen.cmdline = None;
            changed
        };
        // Re-read stat to rule out an exec between the two reads only when
        // something changed: unchanged identity and argv need no third read.
        if named
            && (identity_changed || argv_changed)
            && !seen
                .stat
                .prefix()
                .ok()
                .and_then(crate::util::stat_fields)
                .is_some_and(|(c, p, t)| c == seen.comm && p == ppid && t == start)
        {
            self.forget(pid);
            return;
        }
        if !named || !same {
            self.exits.remove(&pid);
        }
        if named && !self.exits.contains_key(&pid) && !fixture {
            if let Ok(fd) = crate::util::pidfd(pid as i32) {
                self.exits.insert(pid, fd);
                if seen
                    .stat
                    .prefix()
                    .ok()
                    .and_then(crate::util::stat_fields)
                    .is_none_or(|(_, _, t)| t != start)
                {
                    self.forget(pid);
                    return;
                }
            }
        }
        if identity_changed || argv_changed {
            seen.codex = is_codex_native_process(&argv);
            seen.claude = is_claude_native_process(&argv);
            seen.inhibitor = is_codex_active_inhibitor(&argv);
        }
        if let Some(p) = self.procs.get_mut(&pid) {
            p.ppid = ppid as i64;
            p.start_ticks = start;
            p.cmdline = argv;
        } else {
            self.procs.insert(
                pid,
                ProcInfo {
                    ppid: ppid as i64,
                    start_ticks: start,
                    cmdline: argv,
                },
            );
        }
        let agent = named && identity_changed && !fixture;
        self.seen.insert(pid, seen);
        if !known || !same {
            self.fresh_next.push(pid);
        }
        // An agent launched from an unmarked path: watch its binary from now on.
        if agent && self.exec.is_some() {
            if let Some(exe) = proc.exe(pid as i32) {
                self.exec_watch().map(|w| w.mark(&exe));
            }
        }
    }
}
/// kthreadd (PID 2) and its children.
fn is_kthread(procs: &HashMap<i64, ProcInfo>, pid: i64) -> bool {
    pid == 2 || procs.get(&pid).is_some_and(|p| p.ppid == 2)
}
// Match legacy lossy argv decoding without allocating replacement text on
// every unchanged probe when an argument contains non-UTF-8 bytes.
fn lossy_eq(cached: &str, bytes: &[u8]) -> bool {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return cached == text;
    }
    cached.chars().eq(bytes.utf8_chunks().flat_map(|c| {
        c.valid()
            .chars()
            .chain((!c.invalid().is_empty()).then_some(char::REPLACEMENT_CHARACTER))
    }))
}
/// IDs allocated after `last` up to `now` (wrapping at `pid_max`), or `None`
/// when there are too many to stat one by one.
fn new_ids(
    last: i32,
    now: i32,
    pid_max: i32,
) -> Option<std::iter::Chain<std::ops::Range<i64>, std::ops::Range<i64>>> {
    let (last, now, max) = (last as i64, now as i64, pid_max as i64);
    if now >= last {
        (now - last <= MAX_RANGE).then(|| (last + 1..now + 1).chain(0..0))
    } else {
        let n = (max - 1 - last) + now;
        (n <= MAX_RANGE).then(|| (last + 1..max).chain(1..now + 1))
    }
}
/// Everything `sync_process_sessions` derives from the process map: active
/// Codex and Claude PIDs with their start times. Liveness of other session
/// PIDs is observed with pidfds, not here.
pub fn is_codex_native_process(cmdline: &[String]) -> bool {
    let Some(exe) = cmdline.first() else {
        return false;
    };
    if cmdline[1..].iter().any(|a| a.contains("app-server")) {
        return false;
    }
    basename(exe) == "codex"
}
pub fn is_claude_native_process(cmdline: &[String]) -> bool {
    let Some(exe) = cmdline.first() else {
        return false;
    };
    if basename(exe) != "claude" {
        return false;
    }
    const IGNORED: &[&str] = &[
        "--help",
        "-h",
        "--version",
        "-v",
        "help",
        "config",
        "doctor",
        "install",
        "mcp",
        "update",
    ];
    !cmdline
        .iter()
        .skip(1)
        .take(2)
        .any(|a| IGNORED.contains(&a.as_str()))
}
pub fn is_codex_active_inhibitor(cmdline: &[String]) -> bool {
    let Some(exe) = cmdline.first() else {
        return false;
    };
    if basename(exe) != "systemd-inhibit" {
        return false;
    }
    let joined = cmdline.join(" ").to_lowercase();
    cmdline.iter().any(|a| a == "--who")
        && cmdline.iter().any(|a| a == "codex")
        && joined.contains("codex is running an active turn")
}
fn codex_ancestor(pid: i64, procs: &HashMap<i64, ProcInfo>, codex: &HashSet<i64>) -> Option<i64> {
    let mut seen = HashSet::new();
    let mut current = procs.get(&pid).map(|p| p.ppid).unwrap_or(0);
    while current > 0 && seen.insert(current) {
        if codex.contains(&current) {
            return Some(current);
        }
        current = procs.get(&current).map(|p| p.ppid).unwrap_or(0);
    }
    None
}
fn find_session_for_pid(sessions: &Obj, pid: i64, source: &str) -> Option<String> {
    let mut matched = None;
    for (sid, s) in sessions {
        let Some(s) = s.as_object() else { continue };
        if str_of(s, "source") != Some(source) || session_pid(s) != Some(pid) {
            continue;
        }
        if str_of(s, "provider") != Some("process-scan") {
            return Some(sid.clone());
        }
        matched = Some(sid.clone());
    }
    matched
}
/// Codex PIDs in an active turn (an inhibitor descends from them) and Claude PIDs.
fn discover(procs: &HashMap<i64, ProcInfo>) -> (HashSet<i64>, HashSet<i64>) {
    let codex: HashSet<i64> = procs
        .iter()
        .filter(|(_, p)| is_codex_native_process(&p.cmdline))
        .map(|(pid, _)| *pid)
        .collect();
    let mut active: HashSet<i64> = HashSet::new();
    for (pid, p) in procs {
        if is_codex_active_inhibitor(&p.cmdline) {
            if let Some(a) = codex_ancestor(*pid, procs, &codex) {
                active.insert(a);
            }
        }
    }
    let claude: HashSet<i64> = procs
        .iter()
        .filter(|(_, p)| is_claude_native_process(&p.cmdline))
        .map(|(pid, _)| *pid)
        .collect();
    (active, claude)
}
/// `sync_codex_process_sessions` at legacy scan time `now` (the caller's
/// [`ScanClock`]; rate limiting is the caller's lease timer).
///
/// The read-only actor re-applies the last scan on later refreshes instead of
/// persisting it. A session a hook updated after that scan (`last_activity >
/// now`) is left alone: legacy had not scanned since, so it showed the hook's
/// version until the next scan.
pub fn sync_process_sessions(
    state: &mut Value,
    procs: &HashMap<i64, ProcInfo>,
    cwd_of: &dyn Fn(i64) -> Option<String>,
    now: f64,
) {
    let (active, claude) = discover(procs);
    let sessions = sessions_mut(state);
    let mut seen_synthetic = HashSet::new();
    for (source, pids, floor) in [("codex", &active, 0.46), ("claude", &claude, 0.38)] {
        let mut sorted: Vec<i64> = pids.iter().copied().collect();
        sorted.sort();
        for pid in sorted {
            let start_ticks = procs.get(&pid).map(|p| p.start_ticks).unwrap_or(0);
            let cwd = cwd_of(pid).unwrap_or_default();
            let existing = find_session_for_pid(sessions, pid, source);
            let prefix = if source == "codex" {
                "codexproc"
            } else {
                "claudeproc"
            };
            let sid = existing
                .clone()
                .unwrap_or_else(|| format!("{prefix}-{pid}-{start_ticks}"));
            let mut s = match sessions.get(&sid) {
                Some(Value::Object(o)) => o.clone(),
                _ => Obj::new(),
            };
            s.entry("id").or_insert(json!(sid));
            if existing.is_some() && num(s.get("last_activity")) > now {
                if str_of(&s, "provider") == Some("process-scan") {
                    seen_synthetic.insert(sid.clone());
                }
                continue;
            }
            s.insert("source".into(), json!(source));
            s.insert("source_label".into(), json!(source_label(source)));
            let title_default = if source == "codex" {
                format!("Codex {pid}")
            } else {
                format!("Claude {pid}")
            };
            let title = str_of(&s, "title")
                .filter(|t| !t.is_empty())
                .map(str::to_string);
            s.insert("title".into(), json!(title.unwrap_or(title_default)));
            let cwd = if cwd.is_empty() {
                str_of(&s, "cwd").unwrap_or("").to_string()
            } else {
                cwd
            };
            s.insert("project".into(), json!(project_name(Some(&cwd))));
            s.insert("cwd".into(), json!(cwd));
            s.insert("event".into(), json!("ProcessScan"));
            s.insert("status".into(), json!("running"));
            if source == "codex" {
                s.insert("detail".into(), json!("Active turn"));
                s.insert("current_tool".into(), json!("Active turn"));
            } else {
                for k in ["detail", "current_tool"] {
                    if !truthy(s.get(k)) {
                        s.insert(k.into(), json!("Claude Code"));
                    }
                }
            }
            s.insert("had_activity".into(), json!(true));
            s.remove("completed_at");
            let progress = num(s.get("progress")).max(floor);
            s.insert("progress".into(), json!(progress));
            s.insert("last_activity".into(), json!(now));
            s.entry("started_at").or_insert(json!(now));
            s.insert("process_scan_seen_at".into(), json!(now));
            s.insert("process_scan_active_seen_at".into(), json!(now));
            s.insert("process_start_ticks".into(), json!(start_ticks));
            if existing.is_none() {
                s.insert("provider".into(), json!("process-scan"));
            }
            if str_of(&s, "provider") == Some("process-scan") {
                seen_synthetic.insert(sid.clone());
            }
            let mut terminal = match s.get("terminal") {
                Some(Value::Object(t)) => t.clone(),
                _ => Obj::new(),
            };
            terminal.insert("pid".into(), json!(pid));
            s.insert("terminal".into(), Value::Object(terminal));
            sessions.insert(sid, Value::Object(s));
        }
    }
    let ids: Vec<String> = sessions.keys().cloned().collect();
    for sid in ids {
        let Some(Value::Object(s)) = sessions.get_mut(&sid) else {
            continue;
        };
        let pid = session_pid(s);
        let exists = pid.is_some_and(|p| p > 0 && procs.contains_key(&p));
        if str_of(s, "provider") == Some("process-scan")
            && !seen_synthetic.contains(&sid)
            && !exists
        {
            sessions.remove(&sid);
            continue;
        }
        let source = str_of(s, "source").unwrap_or("");
        let scan_active = pid.is_some_and(|p| {
            (source == "codex" && active.contains(&p))
                || (source == "claude" && claude.contains(&p))
        });
        if str_of(s, "event") == Some("ProcessScan") && !scan_active {
            s.insert("current_tool".into(), json!(""));
            if exists && truthy(s.get("had_activity")) {
                s.insert("status".into(), json!("completed"));
                s.insert("detail".into(), json!("Done"));
                s.insert("progress".into(), json!(1.0));
                s.entry("completed_at").or_insert(json!(now));
            } else {
                s.insert("status".into(), json!("idle"));
                s.insert("detail".into(), json!("Idle"));
            }
        }
    }
}

/// PIDs referenced by sessions; the actor watches them with pidfds.
pub fn referenced_pids(state: &Value) -> HashSet<i64> {
    state
        .get("sessions")
        .and_then(Value::as_object)
        .map(|s| {
            s.values()
                .filter_map(Value::as_object)
                .filter_map(session_pid)
                .filter(|p| *p > 0)
                .collect()
        })
        .unwrap_or_default()
}
/// Persisted content excluding bookkeeping timestamps, for change detection.
pub fn content_key(state: &Value) -> String {
    let mut v = state.clone();
    if let Some(o) = v.as_object_mut() {
        o.remove("updated_at");
    }
    v.to_string()
}

// ------------------------------------------------------------ codex app-server

pub const APP_PREFIX: &str = "codexapp:";
/// `status_from_thread_status`.
pub fn status_from_thread_status(status: Option<&Value>) -> (&'static str, String, f64) {
    let Some(st) = status.and_then(Value::as_object) else {
        return ("processing", "Starting".into(), 0.2);
    };
    let kind = st.get("type");
    match kind.and_then(Value::as_str) {
        Some("active") => {
            let flags: Vec<&str> = st
                .get("activeFlags")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if flags.contains(&"waitingOnApproval") {
                ("waitingApproval", "Waiting for approval".into(), 0.82)
            } else if flags.contains(&"waitingOnUserInput") {
                ("waitingQuestion", "Waiting for input".into(), 0.82)
            } else {
                ("running", "Running".into(), 0.46)
            }
        }
        Some("idle") => ("completed", "Idle".into(), 1.0),
        Some(k @ ("systemError" | "notLoaded")) => ("idle", k.into(), 0.0),
        _ => {
            // Python `str(kind or "Working")`.
            let text = match kind {
                Some(Value::String(s)) if !s.is_empty() => s.clone(),
                Some(v) if truthy(Some(v)) => python_str(v),
                _ => "Working".into(),
            };
            ("processing", text, 0.32)
        }
    }
}
fn python_str(v: &Value) -> String {
    match v {
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        other => other.to_string(),
    }
}
fn nonempty_str<'a>(o: &'a Obj, k: &str) -> Option<&'a str> {
    o.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}
/// `update_thread` (`thread/started`).
pub fn update_thread(state: &mut Value, thread: &Obj, now: f64) {
    let Some(tid) = nonempty_str(thread, "id") else {
        return;
    };
    let sid = format!("{APP_PREFIX}{tid}");
    let (status, detail, progress) = status_from_thread_status(thread.get("status"));
    let sessions = sessions_mut(state);
    let mut s = match sessions.get(&sid) {
        Some(Value::Object(o)) => o.clone(),
        _ => Obj::new(),
    };
    s.entry("id").or_insert(json!(sid));
    s.insert("source".into(), json!("codex"));
    s.insert("source_label".into(), json!("Codex"));
    s.insert("provider".into(), json!("app-server"));
    s.insert("provider_session_id".into(), json!(tid));
    let title = nonempty_str(thread, "name")
        .map(str::to_string)
        .unwrap_or_else(|| truncate(tid, 8));
    s.insert("title".into(), json!(title));
    let cwd = match thread.get("cwd") {
        Some(Value::String(c)) => c.clone(),
        _ => match s.get("cwd") {
            Some(v) if truthy(Some(v)) => v
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string()),
            _ => String::new(),
        },
    };
    s.insert("project".into(), json!(project_name(Some(&cwd))));
    s.insert("cwd".into(), json!(cwd));
    let detail = nonempty_str(thread, "preview")
        .map(str::to_string)
        .unwrap_or(detail);
    s.insert("detail".into(), json!(detail));
    if let Some(path) = nonempty_str(thread, "path") {
        s.insert("transcript_path".into(), json!(path));
    }
    s.insert("status".into(), json!(status));
    s.insert("event".into(), json!("app-server"));
    s.insert("progress".into(), json!(progress));
    s.insert("last_activity".into(), json!(now));
    s.entry("started_at").or_insert(json!(now));
    if status == "completed" {
        s.insert("completed_at".into(), json!(now));
    }
    sessions.insert(sid, Value::Object(s));
}
/// `update_thread_status` (`thread/status/changed`); unknown threads are ignored.
pub fn update_thread_status(state: &mut Value, tid: &str, status: Option<&Value>, now: f64) {
    let Some(Value::Object(s)) = sessions_mut(state).get_mut(&format!("{APP_PREFIX}{tid}")) else {
        return;
    };
    let (st, detail, progress) = status_from_thread_status(status);
    s.insert("status".into(), json!(st));
    s.insert("detail".into(), json!(detail));
    s.insert("progress".into(), json!(progress));
    s.insert("last_activity".into(), json!(now));
    if st == "completed" {
        s.insert("completed_at".into(), json!(now));
    }
}
/// `handle_message`: apply one JSON-RPC notification. Returns true if handled.
pub fn apply_app_message(state: &mut Value, message: &Value, now: f64) -> bool {
    let (Some(method), Some(params)) = (
        message.get("method").and_then(Value::as_str),
        message.get("params").and_then(Value::as_object),
    ) else {
        return false;
    };
    match method {
        "thread/started" => {
            if let Some(Value::Object(t)) = params.get("thread") {
                update_thread(state, t, now);
            }
        }
        "thread/status/changed" => {
            if let Some(tid) = params.get("threadId").and_then(Value::as_str) {
                let status = params.get("status").filter(|v| v.is_object());
                update_thread_status(state, tid, status, now);
            }
        }
        "thread/closed" => {
            if let Some(tid) = params.get("threadId").and_then(Value::as_str) {
                sessions_mut(state).remove(&format!("{APP_PREFIX}{tid}"));
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn obj(v: Value) -> Obj {
        v.as_object().unwrap().clone()
    }
    #[test]
    fn hook_event_flow_matches_python() {
        let mut st = default_state(0.0);
        let p = obj(
            json!({"session_id":"abc","cwd":"/home/u/proj","tool_name":"Bash",
            "tool_input":{"description":"List","command":"ls -la"}}),
        );
        let (sid, rid) = update_session_from_event(
            &mut st,
            &p,
            "claude",
            "PreToolUse",
            json!({"pid": 1}),
            1,
            100.0,
        );
        assert_eq!(sid, "abc");
        assert!(rid.is_none());
        let s = &st["sessions"]["abc"];
        assert_eq!(s["status"], "running");
        assert_eq!(s["current_tool"], "Bash");
        assert_eq!(s["detail"], "List: ls -la");
        assert_eq!(s["project"], "proj");
        assert_eq!(s["title"], "abc");
        assert_eq!(s["progress"], 0.42);
        assert_eq!(s["source_label"], "Claude Code");

        let p = obj(
            json!({"session_id":"abc","tool_name":"Read","tool_input":{"file_path":"/a/b.rs"}}),
        );
        update_session_from_event(
            &mut st,
            &p,
            "claude",
            "PermissionRequest",
            json!({"pid":1}),
            1,
            101.0,
        );
        assert_eq!(st["sessions"]["abc"]["status"], "waitingApproval");
        assert_eq!(st["sessions"]["abc"]["detail"], "b.rs");
        assert_eq!(st["pending_requests"].as_object().unwrap().len(), 1);
        let rid = st["sessions"]["abc"]["pending_request_id"]
            .as_str()
            .unwrap()
            .to_string();
        drop_request(&mut st, "abc", &rid);
        assert!(st["pending_requests"].as_object().unwrap().is_empty());

        let p = obj(json!({"session_id":"abc","message":"Claude is waiting for your input"}));
        update_session_from_event(
            &mut st,
            &p,
            "claude",
            "Notification",
            json!({"pid":1}),
            1,
            102.0,
        );
        assert_eq!(st["sessions"]["abc"]["status"], "completed");
        assert_eq!(st["sessions"]["abc"]["progress"], 1.0);
    }
    #[test]
    fn ask_user_question_permission() {
        let mut st = default_state(0.0);
        let p = obj(
            json!({"session_id":"q","tool_name":"AskUserQuestion","tool_use_id":"t1",
            "tool_input":{"questions":[{"question":"Pick?","header":"H","options":[{"label":"A"},"B"]}]}}),
        );
        let (_, rid) = update_session_from_event(
            &mut st,
            &p,
            "claude",
            "PermissionRequest",
            json!({}),
            9,
            5.0,
        );
        assert_eq!(rid.as_deref(), Some("claude-q-t1"));
        let r = &st["pending_requests"]["claude-q-t1"];
        assert_eq!(r["kind"], "question_from_permission");
        assert_eq!(r["answer_key"], "H");
        assert_eq!(r["options"], json!(["A", "B"]));
        assert_eq!(st["sessions"]["q"]["status"], "waitingQuestion");
    }
    #[test]
    fn cleanup_and_expiry() {
        let mut st = default_state(0.0);
        st["sessions"]["done"] =
            json!({"status":"completed","last_activity":100.0,"completed_at":100.0});
        st["sessions"]["run"] = json!({"status":"running","last_activity":100.0});
        st["sessions"]["dead"] =
            json!({"status":"running","last_activity":100.0,"terminal":{"pid":42}});
        st["sessions"]["kept"] = json!({"status":"completed","had_activity":true,"last_activity":1.0,"terminal":{"pid":7}});
        let mut alive = |p: i64| p == 7;
        assert_eq!(next_expiry(&st, &mut alive), Some(120.0));
        cleanup_state(&mut st, 121.0, &mut alive);
        let s = st["sessions"].as_object().unwrap();
        assert!(!s.contains_key("done") && !s.contains_key("dead"));
        assert!(s.contains_key("run") && s.contains_key("kept"));
        cleanup_state(&mut st, 1001.0, &mut alive);
        assert!(!st["sessions"].as_object().unwrap().contains_key("run"));
    }
    #[test]
    fn public_view_sorting_and_idle_hiding() {
        let mut st = default_state(0.0);
        st["sessions"]["a"] =
            json!({"id":"a","status":"running","last_activity":5.0,"event":"PreToolUse"});
        st["sessions"]["b"] = json!({"id":"b","status":"waitingApproval","last_activity":1.0,"event":"PermissionRequest"});
        st["sessions"]["c"] = json!({"id":"c","status":"processing","event":"SessionStart"});
        let (rows, _) = public_sessions(&mut st, 10.0);
        let ids: Vec<_> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        assert_eq!(st["sessions"]["c"]["status"], "idle");
        assert!((display_progress("running", 0.0, 0.0, 6.0) - 0.22).abs() < 1e-9);
        assert!((display_progress("running", 0.42, 0.0, 6.0) - 0.42).abs() < 1e-9);
        assert!((display_progress("running", 0.42, 1.0, 7.0) - 0.55).abs() < 1e-9);
        assert_eq!(display_progress("running", 0.99, 0.0, 6.0), 0.94);
    }
    #[test]
    fn process_scan_synthesizes_and_completes() {
        let mut st = default_state(0.0);
        let mut procs = HashMap::new();
        procs.insert(
            10,
            ProcInfo {
                ppid: 1,
                start_ticks: 77,
                cmdline: vec!["/usr/bin/codex".into()],
            },
        );
        procs.insert(
            11,
            ProcInfo {
                ppid: 10,
                start_ticks: 78,
                cmdline: [
                    "systemd-inhibit",
                    "--who",
                    "codex",
                    "--why",
                    "Codex is running an active turn",
                ]
                .map(String::from)
                .to_vec(),
            },
        );
        procs.insert(
            20,
            ProcInfo {
                ppid: 1,
                start_ticks: 5,
                cmdline: vec!["claude".into(), "--version".into()],
            },
        );
        sync_process_sessions(&mut st, &procs, &|_| Some("/w/x".into()), 50.0);
        let s = &st["sessions"]["codexproc-10-77"];
        assert_eq!(s["status"], "running");
        assert_eq!(s["project"], "x");
        assert_eq!(s["progress"], 0.46);
        assert_eq!(st["sessions"].as_object().unwrap().len(), 1);
        procs.remove(&11);
        sync_process_sessions(&mut st, &procs, &|_| None, 51.0);
        assert_eq!(st["sessions"]["codexproc-10-77"]["status"], "completed");
        assert_eq!(st["sessions"]["codexproc-10-77"]["detail"], "Done");
        procs.remove(&10);
        sync_process_sessions(&mut st, &procs, &|_| None, 52.0);
        assert!(st["sessions"].as_object().unwrap().is_empty());
    }
    fn fake_pid(
        root: &std::path::Path,
        pid: i64,
        comm: &str,
        ppid: i64,
        start: u64,
        argv: &[&str],
    ) {
        let d = root.join(pid.to_string());
        std::fs::create_dir_all(&d).unwrap();
        let stat =
            format!("{pid} ({comm}) S {ppid} 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 {start} 0 0");
        std::fs::write(d.join("stat"), stat).unwrap();
        std::fs::write(d.join("cmdline"), argv.join("\0") + "\0").unwrap();
    }
    #[test]
    fn proc_table_is_incremental_and_reports_discovery_changes() {
        let root = std::env::temp_dir().join(format!("hyprv-proctable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proc = Proc { root: root.clone() };
        fake_pid(&root, 1, "systemd", 0, 1, &["/sbin/init"]);
        fake_pid(&root, 30, "bash", 1, 5, &["bash"]);
        let mut t = ProcTable::default();
        assert!(!t.refresh(&proc, 100.0), "no agents yet");
        assert!(t.agents().is_empty());
        // A new claude PID is found by the readdir diff alone.
        fake_pid(&root, 40, "claude", 30, 9, &["claude"]);
        assert!(t.refresh(&proc, 101.5));
        assert_eq!(t.agents(), &[(40, 9, "claude")]);
        assert!(!t.refresh(&proc, 103.0), "unchanged probe");
        // Fork-then-exec: PID 50 first appears as bash, then becomes claude.
        fake_pid(&root, 50, "bash", 30, 11, &["bash"]);
        assert!(!t.refresh(&proc, 104.0));
        fake_pid(&root, 50, "claude", 30, 11, &["claude"]);
        assert!(t.refresh(&proc, 105.5), "young PIDs are re-read");
        // An old PID that execs later is discovered at the next probe.
        fake_pid(&root, 30, "claude", 1, 5, &["claude"]);
        assert!(
            t.refresh(&proc, 110.0),
            "old non-agent PIDs must observe exec without waiting for the sweep"
        );
        assert!(!t.refresh(&proc, 160.0), "the periodic sweep agrees");
        assert_eq!(t.agents().len(), 3);
        // Exit of an agent PID.
        std::fs::remove_dir_all(root.join("40")).unwrap();
        assert!(t.refresh(&proc, 161.0));
        assert!(!t.procs.contains_key(&40));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn proc_table_range_probe_checks_new_ids_and_cached_identities() {
        let root = std::env::temp_dir().join(format!("hyprv-procrange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let proc = Proc { root: root.clone() };
        let last_pid = |n: i64| {
            std::fs::write(root.join("loadavg"), format!("0.00 0.00 0.00 1/9 {n}\n")).unwrap()
        };
        std::fs::create_dir_all(root.join("sys/kernel")).unwrap();
        std::fs::write(root.join("sys/kernel/pid_max"), "100\n").unwrap();
        fake_pid(&root, 1, "systemd", 0, 1, &["/sbin/init"]);
        fake_pid(&root, 30, "bash", 1, 5, &["bash"]);
        last_pid(30);
        let mut t = ProcTable::default();
        assert!(!t.refresh(&proc, 100.0), "first refresh is a sweep");
        // Not in the readdir order any more: only IDs 31..=33 are looked at.
        fake_pid(&root, 32, "claude", 30, 9, &["claude"]);
        // A thread named like an agent (TID 33 of process 32) is rejected.
        fake_pid(&root, 33, "claude", 30, 9, &["claude"]);
        std::fs::write(root.join("33/status"), "Name:\tclaude\nTgid:\t32\n").unwrap();
        std::fs::write(root.join("32/status"), "Name:\tclaude\nTgid:\t32\n").unwrap();
        // An old-numbered PID that the range does not cover stays unseen.
        fake_pid(&root, 20, "claude", 1, 3, &["claude"]);
        last_pid(33);
        assert!(t.refresh(&proc, 101.5));
        assert_eq!(t.agents(), &[(32, 9, "claude")]);
        assert!(!t.procs.contains_key(&33));
        // Wrap-around: 98..=99 and 1..=2.
        fake_pid(&root, 99, "codex", 1, 12, &["/usr/bin/codex"]);
        std::fs::write(root.join("99/status"), "Name:\tcodex\nTgid:\t99\n").unwrap();
        last_pid(2);
        assert!(
            !t.refresh(&proc, 103.0),
            "a codex without an inhibitor is not active"
        );
        assert!(t.procs.contains_key(&99));
        // The sweep finds PID 20 (a real readdir never lists TID 33).
        std::fs::remove_dir_all(root.join("33")).unwrap();
        assert!(t.refresh(&proc, 161.0));
        assert_eq!(t.agents().len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn proc_table_on_real_proc_finds_new_agent_and_its_exit() {
        let dir = std::env::temp_dir().join(format!("hyprv-procreal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // `sleep` exec'd through a link named `claude`: comm and argv[0] say claude.
        let exe = dir.join("claude");
        std::os::unix::fs::symlink("/usr/bin/sleep", &exe).unwrap();
        let proc = Proc::default();
        let mut t = ProcTable::default();
        let now = crate::util::epoch_f64();
        t.refresh(&proc, now);
        let mut child = std::process::Command::new(&exe).arg("30").spawn().unwrap();
        let pid = child.id() as i64;
        t.refresh(&proc, now + 1.0);
        assert!(t.agents().iter().any(|a| a.0 == pid), "found by the probe");
        child.kill().unwrap();
        child.wait().unwrap();
        t.refresh(&proc, now + 2.0);
        assert!(!t.agents().iter().any(|a| a.0 == pid), "exit seen");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn session_keys_follow_document_order() {
        let doc = br#"{"version":1,"sessions":{"b":{"x":{"y":[1,{"z":2}]}},"a":{},"c":null},"pending_requests":{}}"#;
        assert_eq!(session_keys(doc), ["b", "a", "c"]);
        assert!(session_keys(b"").is_empty());
        assert!(session_keys(br#"{"sessions":null}"#).is_empty());
    }
    #[test]
    fn legacy_order_appends_new_and_readded_sessions() {
        let mut o = LegacyOrder::default();
        let v = |x: &[&str]| x.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        o.update(
            &v(&["h2", "h1"]),
            &v(&["claudeproc-20-1", "codexproc-30-1", "claudeproc-9-1"]),
        );
        fn ranked<'a>(o: &LegacyOrder, ids: &[&'a str]) -> Vec<&'a str> {
            let mut ids = ids.to_vec();
            ids.sort_by_key(|i| o.rank(i));
            ids
        }
        let all = [
            "h1",
            "h2",
            "claudeproc-20-1",
            "claudeproc-9-1",
            "codexproc-30-1",
        ];
        assert_eq!(
            ranked(&o, &all),
            [
                "h2",
                "h1",
                "codexproc-30-1",
                "claudeproc-9-1",
                "claudeproc-20-1"
            ]
        );
        // h2 expires, then a hook re-creates it: it goes to the end.
        o.update(
            &v(&["h1"]),
            &v(&["claudeproc-20-1", "codexproc-30-1", "claudeproc-9-1"]),
        );
        o.update(
            &v(&["h2", "h1"]),
            &v(&["claudeproc-20-1", "codexproc-30-1", "claudeproc-9-1"]),
        );
        assert_eq!(ranked(&o, &all)[4], "h2");
    }
    #[test]
    fn descriptions() {
        assert_eq!(
            tool_description(&obj(
                json!({"tool_name":"Grep","tool_input":{"pattern":"foo"}})
            )),
            "foo"
        );
        assert_eq!(tool_description(&obj(json!({}))), "Working");
        assert_eq!(project_name(Some("/a/b/")), "b");
        assert_eq!(project_name(Some("/")), "/");
        assert_eq!(normalize_source(Some(" Claude-Code ")), Some("claude"));
        assert_eq!(normalize_event(Some("pre_tool_use")), "PreToolUse");
    }
    #[test]
    fn app_server_messages_match_python() {
        let mut st = default_state(0.0);
        let started = json!({"method":"thread/started","params":{"thread":{"id":"0123456789","cwd":"/w/repo",
            "status":{"type":"active","activeFlags":["waitingOnApproval"]},"path":"/t.jsonl"}}});
        assert!(apply_app_message(&mut st, &started, 10.0));
        let s = &st["sessions"]["codexapp:0123456789"];
        assert_eq!(s["title"], "01234567");
        assert_eq!(s["status"], "waitingApproval");
        assert_eq!(s["detail"], "Waiting for approval");
        assert_eq!(s["project"], "repo");
        assert_eq!(s["provider"], "app-server");
        assert_eq!(s["transcript_path"], "/t.jsonl");
        let changed = json!({"method":"thread/status/changed","params":{"threadId":"0123456789","status":{"type":"idle"}}});
        apply_app_message(&mut st, &changed, 11.0);
        assert_eq!(st["sessions"]["codexapp:0123456789"]["status"], "completed");
        assert_eq!(st["sessions"]["codexapp:0123456789"]["completed_at"], 11.0);
        assert_eq!(
            status_from_thread_status(Some(&json!({"type":"notLoaded"}))).1,
            "notLoaded"
        );
        assert_eq!(
            status_from_thread_status(Some(&json!({"type":"weird"}))).1,
            "weird"
        );
        assert_eq!(status_from_thread_status(None).0, "processing");
        apply_app_message(
            &mut st,
            &json!({"method":"thread/status/changed","params":{"threadId":"zzz"}}),
            12.0,
        );
        assert!(st["sessions"].get("codexapp:zzz").is_none());
        apply_app_message(
            &mut st,
            &json!({"method":"thread/closed","params":{"threadId":"0123456789"}}),
            13.0,
        );
        assert!(st["sessions"].as_object().unwrap().is_empty());
        assert!(!apply_app_message(
            &mut st,
            &json!({"id":1,"result":{}}),
            14.0
        ));
    }
}

#[cfg(test)]
#[path = "process_regressions.rs"]
mod process_regressions;

#[cfg(test)]
#[path = "../../../hyprv-core/src/alloc_probe.rs"]
pub(super) mod sample_alloc;
#[cfg(test)]
mod sampling_regressions {
    use super::*;
    #[test]
    fn cached_revalidation_and_periodic_reconcile_allocate_nothing() {
        let dir = std::env::temp_dir().join(format!(
            "hyprv-probe-{}-{}",
            std::process::id(),
            sample_alloc::cpu_ns()
        ));
        std::fs::create_dir_all(dir.join("sys/kernel")).unwrap();
        std::fs::write(dir.join("loadavg"), "0 0 0 1/4 200\n").unwrap();
        std::fs::write(dir.join("sys/kernel/pid_max"), "4096\n").unwrap();
        for (pid, comm, ppid, args) in [
            (30, "codex", 1, "codex\0"),
            (
                31,
                "systemd-inhibit",
                30,
                "systemd-inhibit\0--who\0codex\0--why\0Codex is running an active turn\0",
            ),
            (200, "claude", 1, "claude\0"),
        ] {
            let p = dir.join(pid.to_string());
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(
                p.join("stat"),
                format!("{pid} ({comm}) S {ppid} 1 1 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 50 0 0"),
            )
            .unwrap();
            std::fs::write(p.join("cmdline"), args).unwrap();
        }
        std::fs::write(dir.join("30/cmdline"), b"codex\0--title\0bad\xff\0").unwrap();
        let proc = Proc { root: dir.clone() };
        let mut table = ProcTable::default();
        for i in 0..3 {
            table.refresh(&proc, i as f64 * 1.5);
        }
        let count = sample_alloc::allocations(|| {
            for i in 3..84 {
                assert!(!table.refresh(&proc, i as f64 * 1.5));
            }
        });
        assert_eq!(
            count, 0,
            "stable PID identities, argv and two full sweeps allocated"
        );
        assert_eq!(table.agents().len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[ignore = "manual read-only process discovery CPU benchmark"]
    fn real_probe_cpu() {
        let proc = Proc {
            root: "/proc".into(),
        };
        let mut table = ProcTable::default();
        for i in 0..3 {
            table.refresh(&proc, i as f64 * 1.5);
        }
        let paced = std::env::var_os("HYPRV_PROBE_PACED").is_some();
        let samples = if paced { 40 } else { 500 };
        let mut times = [0u64; 500];
        let mut count = 0;
        for (i, time) in times[..samples].iter_mut().enumerate() {
            if paced {
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
            let begin = sample_alloc::cpu_ns();
            count += sample_alloc::allocations(|| {
                std::hint::black_box(table.refresh(&proc, (i + 3) as f64 * 1.5));
            });
            *time = sample_alloc::cpu_ns() - begin;
        }
        let ns: u64 = times[..samples].iter().sum();
        times[..samples].sort_unstable();
        println!("probe_cpu_ns={} p50_ns={} p99_ns={} samples={} paced={} allocations={} live_pids={} named_agents={}",ns/samples as u64,times[samples/2],times[(samples-1)*99/100],samples,paced,count,table.procs.len(),table.agents().len());
        if std::env::var_os("HYPRV_PROBE_QUIET_NAMESPACE").is_some() {
            assert_eq!(count, 0);
        }
    }
}
