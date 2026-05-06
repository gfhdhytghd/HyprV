#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import re
from pathlib import Path


CONFIG_DIR = Path(os.environ.get("HYPRV_CONFIG_DIR", Path.home() / ".config" / "HyprV")).expanduser()
HOOK = CONFIG_DIR / "quickshell" / "scripts" / "agent-island-hook.py"
PYTHON = "/usr/bin/python3"
MANAGED_MARKER = "agent-island-hook.py"

CLAUDE_EVENTS = {
    "UserPromptSubmit": 5,
    "PreToolUse": 5,
    "PostToolUse": 5,
    "PostToolUseFailure": 5,
    "PermissionRequest": 86400,
    "Stop": 5,
    "SubagentStart": 5,
    "SubagentStop": 5,
    "SessionStart": 5,
    "SessionEnd": 5,
    "Notification": 86400,
    "PreCompact": 5,
}

CODEX_EVENTS = {
    "SessionStart": 5,
    "SessionEnd": 5,
    "UserPromptSubmit": 5,
    "PreToolUse": 5,
    "PostToolUse": 5,
    "PermissionRequest": 86400,
    "Stop": 5,
}


def load_json(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        raise SystemExit(f"Cannot parse JSON: {path}")
    if not isinstance(data, dict):
        raise SystemExit(f"JSON root must be an object: {path}")
    return data


def save_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def command_for(source: str) -> str:
    return f"{PYTHON} {HOOK} --source {source}"


def hook_list_for(source: str, timeout: int) -> list[dict]:
    return [{"type": "command", "command": command_for(source), "timeout": timeout}]


def remove_managed(entries: list) -> list:
    kept = []
    for entry in entries:
        if not isinstance(entry, dict):
            kept.append(entry)
            continue
        hooks = entry.get("hooks")
        if isinstance(hooks, list):
            hooks = [
                hook for hook in hooks
                if not (isinstance(hook, dict) and MANAGED_MARKER in str(hook.get("command", "")))
            ]
            if hooks:
                copy = dict(entry)
                copy["hooks"] = hooks
                kept.append(copy)
            continue
        if MANAGED_MARKER not in str(entry.get("command", "")):
            kept.append(entry)
    return kept


def install_claude() -> None:
    path = Path.home() / ".claude" / "settings.json"
    data = load_json(path)
    hooks = data.get("hooks")
    if not isinstance(hooks, dict):
        hooks = {}
    for event, timeout in CLAUDE_EVENTS.items():
        entries = remove_managed(hooks.get(event, []) if isinstance(hooks.get(event), list) else [])
        entries.append({
            "matcher": "",
            "hooks": hook_list_for("claude", timeout),
        })
        hooks[event] = entries
    data["hooks"] = hooks
    save_json(path, data)


def install_codex_hooks() -> None:
    path = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")).expanduser() / "hooks.json"
    data = load_json(path)
    hooks = data.get("hooks")
    if not isinstance(hooks, dict):
        hooks = {}
    for event, timeout in CODEX_EVENTS.items():
        entries = remove_managed(hooks.get(event, []) if isinstance(hooks.get(event), list) else [])
        entries.append({
            "matcher": "*",
            "hooks": hook_list_for("codex", timeout),
            "statusMessage": "HyprV island",
        })
        hooks[event] = entries
    data["hooks"] = hooks
    save_json(path, data)


def enable_codex_hooks_feature() -> None:
    config_home = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")).expanduser()
    path = config_home / "config.toml"
    text = path.read_text(encoding="utf-8") if path.exists() else ""
    lines = text.splitlines()
    section_re = re.compile(r"^\s*\[([^\]]+)\]\s*$")
    features_start = None
    features_end = len(lines)
    for idx, line in enumerate(lines):
        match = section_re.match(line)
        if not match:
            continue
        if match.group(1).strip() == "features":
            features_start = idx
            features_end = len(lines)
            continue
        if features_start is not None and idx > features_start:
            features_end = idx
            break

    if features_start is None:
        if lines and lines[-1].strip():
            lines.append("")
        lines.extend(["[features]", "codex_hooks = true"])
    else:
        replaced = False
        for idx in range(features_start + 1, features_end):
            if re.match(r"^\s*codex_hooks\s*=", lines[idx]):
                lines[idx] = "codex_hooks = true"
                replaced = True
                break
        if not replaced:
            lines.insert(features_end, "codex_hooks = true")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    if not HOOK.exists():
        raise SystemExit(f"Missing hook script: {HOOK}")
    install_claude()
    install_codex_hooks()
    enable_codex_hooks_feature()
    print("installed=claude,codex")
    print(f"hook={HOOK}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
