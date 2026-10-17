#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess

from agent_island import is_agent_focus_client, locked_state


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
    parser.add_argument("action", choices=["focus"])
    parser.add_argument("id")
    args = parser.parse_args()

    # Copy before making window/terminal calls so a slow focus never blocks hooks.
    with locked_state(write=False) as state:
        session = state.get("sessions", {}).get(args.id)
    return focus_session(session) if isinstance(session, dict) else 1



if __name__ == "__main__":
    raise SystemExit(main())
