#!/usr/bin/env python3

from __future__ import annotations

import contextlib
import fcntl
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any


STATE_VERSION = 1
SUPPORTED_SOURCES = {"codex", "claude"}
SOURCE_ALIASES = {
    "claude-code": "claude",
    "claudecode": "claude",
    "claude_code": "claude",
}
AGENT_FOCUS_CLASS_KEYWORDS = (
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
)
PROCESS_SCAN_INTERVAL_SECONDS = 1.5
ACTIVITY_EVENTS = {
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
}


def state_dir() -> Path:
    raw = os.environ.get("HYPRV_AGENT_ISLAND_DIR", "~/.cache/hyprv/agent-island")
    path = Path(raw).expanduser()
    path.mkdir(parents=True, exist_ok=True)
    (path / "responses").mkdir(parents=True, exist_ok=True)
    return path


def state_file() -> Path:
    return state_dir() / "state.json"


def response_file(request_id: str) -> Path:
    safe = "".join(ch if ch.isalnum() or ch in "._-" else "_" for ch in request_id)
    return state_dir() / "responses" / f"{safe}.json"


def default_state() -> dict[str, Any]:
    return {
        "version": STATE_VERSION,
        "updated_at": time.time(),
        "sessions": {},
        "pending_requests": {},
    }


def read_state() -> dict[str, Any]:
    path = state_file()
    if not path.exists():
        return default_state()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return default_state()
    if not isinstance(data, dict):
        return default_state()
    data.setdefault("version", STATE_VERSION)
    data.setdefault("updated_at", 0)
    data.setdefault("sessions", {})
    data.setdefault("pending_requests", {})
    if not isinstance(data["sessions"], dict):
        data["sessions"] = {}
    if not isinstance(data["pending_requests"], dict):
        data["pending_requests"] = {}
    return data


def write_state(state: dict[str, Any]) -> None:
    state["version"] = STATE_VERSION
    state["updated_at"] = time.time()
    path = state_file()
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as tmp:
        json.dump(state, tmp, ensure_ascii=False, separators=(",", ":"))
        tmp.write("\n")
        tmp_name = tmp.name
    os.replace(tmp_name, path)


@contextlib.contextmanager
def locked_state(write: bool = True):
    lock_path = state_dir() / "state.lock"
    with lock_path.open("a+", encoding="utf-8") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        state = read_state()
        try:
            yield state
        finally:
            if write:
                write_state(state)
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)


def normalize_source(value: str | None) -> str | None:
    if not value:
        return None
    normalized = value.strip().lower()
    normalized = SOURCE_ALIASES.get(normalized, normalized)
    return normalized if normalized in SUPPORTED_SOURCES else None


def source_label(source: str) -> str:
    if source == "codex":
        return "Codex"
    if source == "claude":
        return "Claude Code"
    return source.title()


def project_name(cwd: str | None) -> str:
    if not cwd:
        return "Session"
    name = Path(cwd).name
    return name or cwd


def first_string(data: dict[str, Any], keys: list[str]) -> str | None:
    for key in keys:
        value = data.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()
    return None


def event_name(payload: dict[str, Any], event_arg: str | None = None) -> str | None:
    return first_string(payload, ["hook_event_name", "hookEventName", "event_name", "eventName", "event"]) or event_arg


def normalize_event(name: str | None) -> str:
    if not name:
        return ""
    mapping = {
        "session_start": "SessionStart",
        "sessionEnd": "SessionEnd",
        "session_end": "SessionEnd",
        "userPromptSubmit": "UserPromptSubmit",
        "user_prompt_submit": "UserPromptSubmit",
        "preToolUse": "PreToolUse",
        "pre_tool_use": "PreToolUse",
        "postToolUse": "PostToolUse",
        "post_tool_use": "PostToolUse",
        "post_tool_use_failure": "PostToolUseFailure",
        "permission_request": "PermissionRequest",
        "notification": "Notification",
        "stop": "Stop",
        "pre_compact": "PreCompact",
        "subagent_start": "SubagentStart",
        "subagent_stop": "SubagentStop",
    }
    return mapping.get(name, name)


def nested_dict(payload: dict[str, Any], keys: list[str]) -> dict[str, Any] | None:
    for key in keys:
        value = payload.get(key)
        if isinstance(value, dict):
            return value
    return None


def tool_name(payload: dict[str, Any]) -> str | None:
    value = first_string(payload, ["tool_name", "toolName", "tool", "name"])
    if value:
        return value
    nested = nested_dict(payload, ["tool", "payload", "data"])
    if nested:
        return first_string(nested, ["name", "tool_name", "toolName"])
    return None


def tool_input(payload: dict[str, Any]) -> dict[str, Any] | None:
    value = nested_dict(payload, ["tool_input", "toolInput", "input", "arguments", "args", "params"])
    if value:
        return value
    nested = nested_dict(payload, ["tool", "payload", "data"])
    if nested:
        return nested_dict(nested, ["input", "tool_input", "toolInput", "arguments", "args", "params"])
    return None


def tool_use_id(payload: dict[str, Any]) -> str | None:
    value = first_string(payload, ["tool_use_id", "toolUseId"])
    if value:
        return value
    nested = nested_dict(payload, ["tool", "tool_use", "toolUse", "payload", "data"])
    if nested:
        return first_string(nested, ["id", "tool_use_id", "toolUseId"])
    return None


def tool_description(payload: dict[str, Any]) -> str:
    name = tool_name(payload) or ""
    inp = tool_input(payload) or {}
    if name == "Bash":
        desc = inp.get("description")
        cmd = inp.get("command")
        if isinstance(desc, str) and desc.strip() and isinstance(cmd, str) and cmd.strip() and desc.strip() != cmd.strip():
            return f"{desc.strip()}: {cmd.strip()[:80]}"
        if isinstance(desc, str) and desc.strip():
            return desc.strip()
        if isinstance(cmd, str) and cmd.strip():
            return cmd.strip()[:100]
    for key in ("file_path", "path"):
        value = inp.get(key)
        if isinstance(value, str) and value.strip():
            return Path(value).name or value.strip()
    for key in ("pattern", "query", "prompt", "command", "description", "message", "text", "content"):
        value = inp.get(key)
        if isinstance(value, str) and value.strip():
            return value.strip()[:120]
    value = first_string(payload, ["message", "text", "summary", "status", "detail", "content", "prompt"])
    if value:
        return value[:120]
    return name or "Working"


def session_id(payload: dict[str, Any], source: str) -> str:
    explicit = first_string(payload, ["session_id", "sessionId"])
    if explicit:
        return explicit
    cwd = first_string(payload, ["cwd"]) or ""
    ppid = os.getppid()
    return f"{source}-ppid-{ppid}-{abs(hash(cwd)) % 100000}"


def session_title(payload: dict[str, Any], session_id_value: str) -> str | None:
    return first_string(payload, ["session_title", "title", "name"]) or session_id_value[:8]


def active_window_snapshot() -> dict[str, Any] | None:
    try:
        out = subprocess.run(
            ["hyprctl", "activewindow", "-j"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=0.4,
        ).stdout.strip()
        if not out:
            return None
        data = json.loads(out)
        if not isinstance(data, dict):
            return None
        return {
            "address": data.get("address") or "",
            "class": data.get("class") or data.get("initialClass") or "",
            "initialClass": data.get("initialClass") or "",
            "title": data.get("title") or data.get("initialTitle") or "",
            "pid": data.get("pid") or 0,
        }
    except Exception:
        return None


def is_agent_focus_client(client: dict[str, Any] | None) -> bool:
    if not isinstance(client, dict):
        return False
    class_text = f"{client.get('class') or ''} {client.get('initialClass') or ''}".lower()
    return any(keyword in class_text for keyword in AGENT_FOCUS_CLASS_KEYWORDS)


def terminal_meta(payload: dict[str, Any]) -> dict[str, Any]:
    meta: dict[str, Any] = {}
    for env_key, out_key in (
        ("TERM_PROGRAM", "term_app"),
        ("TERM_PROGRAM_VERSION", "term_version"),
        ("__CFBundleIdentifier", "term_bundle"),
        ("KITTY_WINDOW_ID", "kitty_window"),
        ("KITTY_LISTEN_ON", "kitty_listen_on"),
        ("TMUX_PANE", "tmux_pane"),
        ("TMUX", "tmux"),
    ):
        value = os.environ.get(env_key)
        if value:
            meta[out_key] = value
    window = active_window_snapshot()
    if window and window.get("address") and is_agent_focus_client(window):
        meta["window"] = window
    if payload.get("_ppid"):
        meta["pid"] = payload.get("_ppid")
    else:
        meta["pid"] = os.getppid()
    return meta


def clean_for_json(value: Any) -> Any:
    if isinstance(value, dict):
        return {str(k): clean_for_json(v) for k, v in value.items()}
    if isinstance(value, list):
        return [clean_for_json(v) for v in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)


def response_allow() -> dict[str, Any]:
    return {
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {"behavior": "allow"},
        }
    }


def response_deny(message: str | None = None) -> dict[str, Any]:
    decision: dict[str, Any] = {"behavior": "deny"}
    if message:
        decision["message"] = message
    return {
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision,
        }
    }


def response_answer_notification(answer: str) -> dict[str, Any]:
    return {
        "hookSpecificOutput": {
            "hookEventName": "Notification",
            "answer": answer,
        }
    }


def response_answer_permission(answer_key: str, answer: str) -> dict[str, Any]:
    return {
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {
                "behavior": "allow",
                "updatedInput": {"answers": {answer_key: answer}},
            },
        }
    }


def extract_question(payload: dict[str, Any]) -> dict[str, Any] | None:
    if isinstance(payload.get("question"), str) and payload["question"].strip():
        options = payload.get("options")
        return {
            "question": payload["question"].strip(),
            "options": options if isinstance(options, list) else [],
            "answer_key": "answer",
            "from_permission": False,
        }
    inp = tool_input(payload) or {}
    if isinstance(inp.get("questions"), list) and inp["questions"]:
        first = inp["questions"][0]
        if isinstance(first, dict):
            question = first.get("question")
            if isinstance(question, str) and question.strip():
                raw_options = first.get("options")
                options: list[str] = []
                if isinstance(raw_options, list):
                    for option in raw_options:
                        if isinstance(option, str):
                            options.append(option)
                        elif isinstance(option, dict) and isinstance(option.get("label"), str):
                            options.append(option["label"])
                answer_key = first.get("header") if isinstance(first.get("header"), str) else "answer_1"
                return {
                    "question": question.strip(),
                    "options": options,
                    "answer_key": answer_key,
                    "from_permission": True,
                }
    if isinstance(inp.get("question"), str) and inp["question"].strip():
        raw_options = inp.get("options")
        options = []
        if isinstance(raw_options, list):
            for option in raw_options:
                if isinstance(option, str):
                    options.append(option)
                elif isinstance(option, dict) and isinstance(option.get("label"), str):
                    options.append(option["label"])
        return {
            "question": inp["question"].strip(),
            "options": options,
            "answer_key": "answer",
            "from_permission": True,
        }
    return None


def is_done_notification(source: str, detail: str) -> bool:
    lowered = detail.strip().lower()
    if source == "claude" and "waiting for your input" in lowered:
        return True
    return False


def status_priority(status: str) -> int:
    return {
        "waitingApproval": 5,
        "waitingQuestion": 4,
        "running": 3,
        "processing": 2,
        "completed": 1,
        "idle": 0,
    }.get(status, 0)


def proc_cmdline(pid: int) -> list[str]:
    try:
        raw = Path(f"/proc/{pid}/cmdline").read_bytes()
    except Exception:
        return []
    return [part.decode("utf-8", errors="replace") for part in raw.split(b"\0") if part]


def proc_stat(pid: int) -> tuple[int, int] | None:
    try:
        text = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8", errors="replace")
    except Exception:
        return None
    close_index = text.rfind(")")
    if close_index < 0:
        return None
    fields = text[close_index + 2 :].split()
    if len(fields) <= 19:
        return None
    try:
        return int(fields[1]), int(fields[19])
    except ValueError:
        return None


def proc_cwd(pid: int) -> str:
    try:
        return os.readlink(f"/proc/{pid}/cwd")
    except Exception:
        return ""


def list_processes() -> dict[int, dict[str, Any]]:
    processes: dict[int, dict[str, Any]] = {}
    try:
        entries = os.listdir("/proc")
    except Exception:
        return processes
    for entry in entries:
        if not entry.isdigit():
            continue
        pid = int(entry)
        cmdline = proc_cmdline(pid)
        stat_data = proc_stat(pid)
        if not cmdline or not stat_data:
            continue
        ppid, start_ticks = stat_data
        processes[pid] = {
            "cmdline": cmdline,
            "ppid": ppid,
            "start_ticks": start_ticks,
        }
    return processes


def is_codex_native_process(cmdline: list[str]) -> bool:
    if not cmdline:
        return False
    if any(arg == "app-server" or "app-server" in arg for arg in cmdline[1:]):
        return False
    executable = cmdline[0]
    name = Path(executable).name
    return name == "codex" and ("/codex/codex" in executable or "@openai/codex" in executable)


def is_codex_active_inhibitor(cmdline: list[str]) -> bool:
    if not cmdline:
        return False
    if Path(cmdline[0]).name != "systemd-inhibit":
        return False
    joined = " ".join(cmdline).lower()
    return "--who" in cmdline and "codex" in cmdline and "codex is running an active turn" in joined


def codex_ancestor_pid(pid: int, processes: dict[int, dict[str, Any]], codex_pids: set[int]) -> int | None:
    seen: set[int] = set()
    current = int(processes.get(pid, {}).get("ppid") or 0)
    while current > 0 and current not in seen:
        if current in codex_pids:
            return current
        seen.add(current)
        current = int(processes.get(current, {}).get("ppid") or 0)
    return None


def int_value(value: Any) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def process_exists(pid: int | None, processes: dict[int, dict[str, Any]] | None = None) -> bool:
    if pid is None or pid <= 0:
        return False
    if processes is not None:
        return pid in processes
    return Path(f"/proc/{pid}").exists()


def session_pid(session: dict[str, Any]) -> int | None:
    terminal = session.get("terminal") if isinstance(session.get("terminal"), dict) else {}
    return int_value(terminal.get("pid"))


def find_session_for_pid(sessions: dict[str, Any], pid: int) -> str | None:
    matched: str | None = None
    for sid, session in sessions.items():
        if not isinstance(session, dict):
            continue
        if session.get("source") != "codex":
            continue
        terminal = session.get("terminal")
        if isinstance(terminal, dict) and int_value(terminal.get("pid")) == pid:
            if session.get("provider") != "process-scan":
                return sid
            matched = sid
    return matched


def sync_codex_process_sessions(state: dict[str, Any]) -> None:
    now = time.time()
    try:
        last_scan = float(state.get("last_process_scan") or 0)
    except (TypeError, ValueError):
        last_scan = 0
    if now - last_scan < PROCESS_SCAN_INTERVAL_SECONDS:
        return

    state["last_process_scan"] = now
    processes = list_processes()
    codex_pids = {
        pid
        for pid, process in processes.items()
        if is_codex_native_process(process.get("cmdline") or [])
    }
    active_pids: set[int] = set()
    for pid, process in processes.items():
        if not is_codex_active_inhibitor(process.get("cmdline") or []):
            continue
        ancestor = codex_ancestor_pid(pid, processes, codex_pids)
        if ancestor is not None:
            active_pids.add(ancestor)

    sessions = state.setdefault("sessions", {})
    seen_synthetic: set[str] = set()

    for pid in sorted(active_pids):
        process = processes.get(pid) or {}
        start_ticks = int(process.get("start_ticks") or 0)
        cwd = proc_cwd(pid)
        existing_sid = find_session_for_pid(sessions, pid)
        sid = existing_sid or f"codexproc-{pid}-{start_ticks}"
        session = sessions.get(sid) if isinstance(sessions.get(sid), dict) else {}

        session.setdefault("id", sid)
        session["source"] = "codex"
        session["source_label"] = source_label("codex")
        session["title"] = session.get("title") or f"Codex {pid}"
        session["cwd"] = cwd or session.get("cwd") or ""
        session["project"] = project_name(session.get("cwd"))
        session["event"] = "ProcessScan"
        session["status"] = "running"
        session["detail"] = "Active turn"
        session["current_tool"] = "Active turn"
        session["had_activity"] = True
        session.pop("completed_at", None)
        try:
            existing_progress = float(session.get("progress") or 0)
        except (TypeError, ValueError):
            existing_progress = 0
        session["progress"] = max(existing_progress, 0.46)
        session["last_activity"] = now
        session.setdefault("started_at", now)
        session["process_scan_seen_at"] = now
        session["process_scan_active_seen_at"] = now
        session["process_start_ticks"] = start_ticks
        if existing_sid is None:
            session["provider"] = "process-scan"
        if session.get("provider") == "process-scan":
            seen_synthetic.add(sid)

        terminal = session.get("terminal") if isinstance(session.get("terminal"), dict) else {}
        terminal["pid"] = pid
        session["terminal"] = clean_for_json(terminal)
        sessions[sid] = clean_for_json(session)

    for sid, session in list(sessions.items()):
        if not isinstance(session, dict):
            continue
        if session.get("provider") == "process-scan" and sid not in seen_synthetic:
            pid_value = session_pid(session)
            if not process_exists(pid_value, processes):
                sessions.pop(sid, None)
                continue
        pid_value = session_pid(session)
        if session.get("event") == "ProcessScan" and pid_value not in active_pids:
            session["current_tool"] = ""
            if process_exists(pid_value, processes) and session.get("had_activity"):
                session["status"] = "completed"
                session["detail"] = "Done"
                session["progress"] = 1.0
                session.setdefault("completed_at", now)
            else:
                session["status"] = "idle"
                session["detail"] = "Idle"


def update_session_from_event(state: dict[str, Any], payload: dict[str, Any], source: str, event: str) -> tuple[dict[str, Any], dict[str, Any] | None]:
    now = time.time()
    sid = session_id(payload, source)
    sessions = state.setdefault("sessions", {})
    session = sessions.get(sid) if isinstance(sessions.get(sid), dict) else {}
    session.setdefault("id", sid)
    session["source"] = source
    session["source_label"] = source_label(source)
    session["title"] = session_title(payload, sid)
    cwd = first_string(payload, ["cwd"]) or session.get("cwd") or ""
    session["cwd"] = cwd
    session["project"] = project_name(cwd)
    if first_string(payload, ["model"]):
        session["model"] = first_string(payload, ["model"])
    session["event"] = event
    session["last_activity"] = now
    session.setdefault("started_at", now)
    meta = terminal_meta(payload)
    if meta:
        session["terminal"] = clean_for_json(meta)
    if event in ACTIVITY_EVENTS:
        session["had_activity"] = True
    elif event == "SessionStart":
        session.setdefault("had_activity", False)

    request: dict[str, Any] | None = None
    detail = tool_description(payload)
    name = tool_name(payload)

    if event == "SessionStart":
        session["status"] = "idle"
        session["detail"] = "Session started"
        session["progress"] = 0.0
    elif event == "UserPromptSubmit":
        session["status"] = "processing"
        session["current_tool"] = ""
        session["detail"] = first_string(payload, ["prompt", "user_prompt", "message", "input"]) or "Reading prompt"
        session["progress"] = 0.16
    elif event in ("PreToolUse", "SubagentStart"):
        session["status"] = "running"
        session["current_tool"] = name or ("Agent" if event == "SubagentStart" else "Tool")
        session["detail"] = detail
        session["progress"] = 0.42
    elif event in ("PostToolUse", "PostToolUseFailure", "SubagentStop"):
        history = session.setdefault("tool_history", [])
        if isinstance(history, list) and name:
            history.append({
                "tool": name,
                "detail": detail,
                "success": event != "PostToolUseFailure",
                "time": now,
            })
            del history[:-8]
        session["status"] = "processing"
        session["current_tool"] = ""
        session["detail"] = f"Finished {name or 'tool'}"
        session["progress"] = 0.66
    elif event == "PermissionRequest":
        question = extract_question(payload) if name == "AskUserQuestion" else None
        request_id = f"{source}-{sid}-{tool_use_id(payload) or int(now * 1000)}"
        if question:
            session["status"] = "waitingQuestion"
            session["current_tool"] = "AskUserQuestion"
            session["detail"] = question["question"]
            request = {
                "id": request_id,
                "kind": "question_from_permission",
                "session_id": sid,
                "source": source,
                "question": question["question"],
                "options": question["options"],
                "answer_key": question["answer_key"],
                "created_at": now,
            }
        else:
            session["status"] = "waitingApproval"
            session["current_tool"] = name or "Permission"
            session["detail"] = detail
            request = {
                "id": request_id,
                "kind": "approval",
                "session_id": sid,
                "source": source,
                "tool": name or "Permission",
                "detail": detail,
                "created_at": now,
            }
        session["pending_request_id"] = request_id
        session["progress"] = 0.82
    elif event == "Notification":
        question = extract_question(payload)
        if question:
            request_id = f"{source}-{sid}-{int(now * 1000)}"
            session["status"] = "waitingQuestion"
            session["current_tool"] = "Question"
            session["detail"] = question["question"]
            request = {
                "id": request_id,
                "kind": "question",
                "session_id": sid,
                "source": source,
                "question": question["question"],
                "options": question["options"],
                "answer_key": question["answer_key"],
                "created_at": now,
            }
            session["pending_request_id"] = request_id
        else:
            session["detail"] = detail
            if is_done_notification(source, detail):
                session["status"] = "completed"
                session["current_tool"] = ""
                session["progress"] = 1.0
                session["completed_at"] = now
                session.pop("pending_request_id", None)
            elif session.get("status") in (None, "idle"):
                session["status"] = "processing"
            session["progress"] = max(float(session.get("progress", 0.5) or 0.5), 0.5)
    elif event == "PreCompact":
        session["status"] = "processing"
        session["current_tool"] = ""
        session["detail"] = "Compacting context"
        session["progress"] = 0.55
    elif event == "Stop":
        session["status"] = "completed"
        session["current_tool"] = ""
        session["detail"] = first_string(payload, ["last_assistant_message", "text", "message", "summary"]) or "Finished"
        session["progress"] = 1.0
        session["completed_at"] = now
        session.pop("pending_request_id", None)
    elif event == "SessionEnd":
        session["status"] = "completed"
        session["current_tool"] = ""
        session["detail"] = "Session ended"
        session["progress"] = 1.0
        session["completed_at"] = now
        session.pop("pending_request_id", None)
    else:
        session["status"] = session.get("status") or "processing"
        session["detail"] = detail

    sessions[sid] = clean_for_json(session)
    if request:
        state.setdefault("pending_requests", {})[request["id"]] = clean_for_json(request)
    return session, request


def cleanup_state(state: dict[str, Any]) -> None:
    now = time.time()
    sessions = state.setdefault("sessions", {})
    for sid, session in list(sessions.items()):
        if not isinstance(session, dict):
            sessions.pop(sid, None)
            continue
        status = session.get("status")
        pid = session_pid(session)
        has_pending = bool(session.get("pending_request_id"))
        if pid is not None and not process_exists(pid) and not has_pending:
            sessions.pop(sid, None)
            continue
        if status == "completed" and session.get("had_activity") and process_exists(pid):
            continue
        last = float(session.get("last_activity") or 0)
        completed = float(session.get("completed_at") or last or 0)
        stale_after = 20 if status in ("completed", "idle") else 900
        if now - max(last, completed) > stale_after:
            sessions.pop(sid, None)

    pending = state.setdefault("pending_requests", {})
    for rid, request in list(pending.items()):
        if not isinstance(request, dict):
            pending.pop(rid, None)
            continue
        sid = request.get("session_id")
        created = float(request.get("created_at") or 0)
        if sid not in sessions or now - created > 86400:
            pending.pop(rid, None)


def public_snapshot(state: dict[str, Any]) -> dict[str, Any]:
    sync_codex_process_sessions(state)
    cleanup_state(state)
    sessions = []
    for session in state.get("sessions", {}).values():
        if not isinstance(session, dict):
            continue
        if session.get("event") == "SessionStart" and not session.get("pending_request_id"):
            session["status"] = "idle"
            session["detail"] = "Session started"
        if session.get("event") == "Notification" and is_done_notification(
            str(session.get("source") or ""),
            str(session.get("detail") or ""),
        ):
            session["status"] = "completed"
            session["current_tool"] = ""
            session["progress"] = 1.0
            session.setdefault("completed_at", time.time())
        status = str(session.get("status") or "idle")
        if status == "idle":
            continue
        item = dict(session)
        base_progress = float(item.get("progress") or 0)
        if status in ("running", "processing"):
            phase = ((time.time() - float(item.get("last_activity") or time.time())) % 12.0) / 12.0
            item["progress"] = min(0.94, max(base_progress, 0.22 + phase * 0.66))
        sessions.append(item)
    sessions.sort(key=lambda s: (status_priority(str(s.get("status") or "")), float(s.get("last_activity") or 0)), reverse=True)

    pending = []
    for request in state.get("pending_requests", {}).values():
        if isinstance(request, dict):
            pending.append(dict(request))
    pending.sort(key=lambda r: float(r.get("created_at") or 0))

    return {
        "version": STATE_VERSION,
        "active": bool(sessions),
        "count": len(sessions),
        "sessions": sessions,
        "pending": pending[0] if pending else None,
        "pending_count": len(pending),
        "updated_at": state.get("updated_at", 0),
    }
