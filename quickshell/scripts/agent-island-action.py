#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

from agent_island import (
    is_agent_focus_client,
    locked_state,
    response_allow,
    response_answer_notification,
    response_answer_permission,
    response_deny,
    response_file,
)


def current_request(state: dict, request_id: str | None) -> dict | None:
    pending = state.get("pending_requests", {})
    if request_id and request_id != "current":
        req = pending.get(request_id)
        return req if isinstance(req, dict) else None
    items = [req for req in pending.values() if isinstance(req, dict)]
    items.sort(key=lambda req: float(req.get("created_at") or 0))
    return items[0] if items else None


def write_response(request_id: str, response: dict) -> None:
    path = response_file(request_id)
    path.write_text(json.dumps(response, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")


def rofi_answer(prompt: str, options: list[str]) -> str | None:
    rofi = "rofi-wayland"
    try:
        subprocess.run([rofi, "-version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=0.5)
    except Exception:
        rofi = "rofi"
    input_text = "\n".join(options) + ("\n" if options else "")
    try:
        proc = subprocess.run(
            [rofi, "-dmenu", "-p", prompt[:80]],
            input=input_text,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=3600,
        )
    except Exception:
        return None
    if proc.returncode != 0:
        return None
    answer = proc.stdout.strip()
    return answer or None


def hypr_clients() -> list[dict]:
    try:
        raw = subprocess.run(
            ["hyprctl", "clients", "-j"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=0.7,
        ).stdout
        data = json.loads(raw)
    except Exception:
        return []
    return data if isinstance(data, list) else []


def focus_address(address: str) -> None:
    subprocess.run(["hyprctl", "dispatch", "focuswindow", f"address:{address}"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def kitty_focus_session(session: dict) -> bool:
    terminal = session.get("terminal") if isinstance(session.get("terminal"), dict) else {}
    kitty_window = terminal.get("kitty_window")
    if not isinstance(kitty_window, str) or not kitty_window.strip():
        return False
    listen_on = terminal.get("kitty_listen_on")
    if not isinstance(listen_on, str) or not listen_on.strip():
        return False

    try:
        proc = subprocess.run(
            [
                "kitty",
                "@",
                "--to",
                listen_on.strip(),
                "focus-window",
                "--match",
                f"id:{kitty_window.strip()}",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=0.8,
        )
    except Exception:
        return False
    return proc.returncode == 0


def session_terms(session: dict) -> list[str]:
    terms: list[str] = []
    for value in (
        session.get("cwd"),
        session.get("project"),
        session.get("title"),
        session.get("source_label"),
        session.get("source"),
    ):
        if isinstance(value, str) and value.strip():
            terms.append(value.strip())
    cwd = session.get("cwd")
    if isinstance(cwd, str) and cwd.strip():
        name = Path(cwd).name
        if name:
            terms.append(name)

    unique: list[str] = []
    seen: set[str] = set()
    for term in terms:
        key = term.lower()
        if key not in seen:
            seen.add(key)
            unique.append(term)
    return unique


def client_score(client: dict, terms: list[str]) -> int:
    if not is_agent_focus_client(client):
        return -1
    title = str(client.get("title") or client.get("initialTitle") or "").lower()
    class_text = f"{client.get('class') or ''} {client.get('initialClass') or ''}".lower()
    score = 5
    if any(term in class_text for term in ("ghostty", "alacritty", "kitty", "wezterm", "foot", "konsole", "terminal")):
        score += 15
    for term in terms:
        lowered = term.lower()
        if len(lowered) < 3:
            continue
        if lowered in title:
            score += 40
        if lowered in class_text:
            score += 10
    return score


def focus_session(session: dict) -> int:
    window = ((session.get("terminal") or {}).get("window") or {}) if isinstance(session.get("terminal"), dict) else {}
    clients = hypr_clients()
    address = window.get("address")
    if isinstance(address, str) and address:
        matched = next((client for client in clients if client.get("address") == address), None)
        if is_agent_focus_client(matched):
            focus_address(address)
            kitty_focus_session(session)
            return 0

    if kitty_focus_session(session):
        return 0

    terms = session_terms(session)
    scored = [(client_score(client, terms), client) for client in clients]
    scored = [(score, client) for score, client in scored if score >= 25 and isinstance(client.get("address"), str)]
    scored.sort(key=lambda item: item[0], reverse=True)
    if scored:
        focus_address(scored[0][1]["address"])
        kitty_focus_session(session)
        return 0
    return 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["approve", "deny", "answer", "reply", "dismiss", "focus"])
    parser.add_argument("id", nargs="?", default="current")
    parser.add_argument("value", nargs="?", default="")
    args = parser.parse_args()

    with locked_state(write=True) as state:
        if args.action == "focus":
            session = state.get("sessions", {}).get(args.id)
            return focus_session(session) if isinstance(session, dict) else 1

        request = current_request(state, args.id)
        if not request:
            return 1
        request_id = str(request["id"])

        if args.action == "dismiss":
            state.get("pending_requests", {}).pop(request_id, None)
            sid = request.get("session_id")
            if sid in state.get("sessions", {}):
                state["sessions"][sid]["status"] = "processing"
                state["sessions"][sid]["pending_request_id"] = ""
                state["sessions"][sid]["last_activity"] = time.time()
            return 0

        if args.action == "approve":
            response = response_allow()
        elif args.action == "deny":
            response = response_deny("Denied from HyprV island")
        else:
            answer = args.value
            if args.action == "reply":
                question = str(request.get("question") or "Answer")
                options = request.get("options") if isinstance(request.get("options"), list) else []
                answer = rofi_answer(question, [str(v) for v in options]) or ""
            if not answer:
                return 1
            if request.get("kind") == "question_from_permission":
                response = response_answer_permission(str(request.get("answer_key") or "answer"), answer)
            else:
                response = response_answer_notification(answer)

        write_response(request_id, response)
        sid = request.get("session_id")
        if sid in state.get("sessions", {}):
            state["sessions"][sid]["status"] = "running" if args.action != "deny" else "idle"
            state["sessions"][sid]["pending_request_id"] = ""
            state["sessions"][sid]["last_activity"] = time.time()
        state.get("pending_requests", {}).pop(request_id, None)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
