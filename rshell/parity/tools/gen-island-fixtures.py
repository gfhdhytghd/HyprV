#!/usr/bin/env python3
"""Regenerate parity/fixtures/island/*.json (docs/spec/ui-island.md §10).

The JSON files are the source of truth for capture.sh and the Rust diff; this
script only exists so the 30-odd fixtures stay consistent. Coordinates in
hover/press steps are island-local logical px (see ui-island.md §2-§4).
"""
import json
import sys
from pathlib import Path

SPECTRUM = [12, 38, 74, 92, 66, 41, 58, 80, 95, 61, 33, 47, 70, 52, 28, 15]

MUSIC = {
    "mediaAvailable": True,
    "mediaPlaying": True,
    "mediaTitle": "Midnight City",
    "mediaArtist": "M83",
    "mediaPlayerName": "spotify",
    "mediaArtUrl": "@asset:cover-warm.png",
    "mediaLengthSeconds": 225,
    "mediaPositionSeconds": 83,
    "spectrumValues": SPECTRUM,
}
NO_MUSIC = {
    "mediaAvailable": False,
    "mediaPlaying": False,
    "mediaTitle": "",
    "mediaArtist": "",
    "mediaPlayerName": "",
    "mediaArtUrl": "",
    "mediaLengthSeconds": 0,
    "mediaPositionSeconds": 0,
    "spectrumValues": [],
}


def agent(i, status, source="codex", progress=0.5, project=None, tool="", detail=""):
    return {
        "id": f"{source}-session-{i}",
        "source": source,
        "source_label": {"codex": "Codex", "claude": "Claude Code"}.get(source, source.title()),
        "title": f"Session {i}",
        "project": project if project is not None else ["rshell", "quickshell", "hypr", "notes", "dotfiles",
                                                        "website", "infra", "paper", "scratch"][i % 9],
        "status": status,
        "current_tool": tool,
        "detail": detail,
        "progress": progress,
    }


RUNNING = agent(0, "running", tool="Bash", progress=0.58, detail="cargo build")
APPROVAL = agent(0, "waitingApproval", tool="Bash", progress=0.82, detail="rm -rf target")
QUESTION = agent(0, "waitingQuestion", source="claude", tool="AskUserQuestion", progress=0.82, detail="Which crate?")
DONE = agent(0, "completed", progress=1.0, detail="Finished")
PROCESSING = agent(0, "processing", source="claude", progress=0.34)

COLLAPSED_H = 58  # bar strip height


def canvas(h):
    return {"width": 640, "height": h}


def expanded_agent_height(n):
    content = 42 * n + 8 * max(0, n - 1)
    return max(184, 104 + max(42, min(content, 386)))


def hover(x, y):
    return {"hover": {"item": "island", "x": x, "y": y}}


AWAY = {"hover": [2, 2]}

fixtures = {}


def fx(name, props, steps=None, h=COLLAPSED_H, note=""):
    base = {"mediaManuallyHidden": False, "agentSessions": [], "now": "09:41"}
    base.update(NO_MUSIC)
    base.update(props)
    fixtures[name] = {
        "component": "island",
        "scene": "island.qml",
        "spec": "docs/spec/ui-island.md",
        "note": note,
        "canvas": canvas(h),
        "fonts": ["JetBrainsMono Nerd Font"],  # CJK fallback (HarmonyOS Sans SC) is checked via fontconfig in capture.sh
        "props": base,
        "steps": steps if steps is not None else [{"capture": ""}],
    }


# 1-2 idle
fx("idle", {}, note="§10.1 no media, no agent")
fx("idle-hover-lock", {}, [hover(16, 19), {"advance": 300}, {"capture": ""}],
   note="§10.2 pointer over lock (settles whether the z-99 hotspot blocks the hover fill)")
fx("idle-hover-power", {}, [hover(100, 19), {"advance": 300}, {"capture": ""}], note="§10.2")
fx("idle-press-lock", {}, [{"press": {"item": "island", "x": 16, "y": 19}},
                            {"frames": {"name": "press", "at": [0, 60, 120, 300]}}],
   note="§10.2 press scale 0.96 (120 ms OutCubic); pointer stays pressed")

# 3-5 music compact
fx("music-compact-playing", MUSIC, note="§10.3")
fx("music-compact-paused", {**MUSIC, "mediaPlaying": False, "spectrumValues": []},
   [{"frames": {"name": "wave", "at": [0, 66, 132]}}], note="§10.4 quiet idle wave, 66 ms timer steps")
fx("music-compact-playing-nodata", {**MUSIC, "spectrumValues": []},
   [{"frames": {"name": "wave", "at": [0, 66]}}], note="§7 playing without cava data: wave in accent colour")
fx("music-compact-noart", {**MUSIC, "mediaArtUrl": ""}, note="§10.5 fallback U+F001")
fx("music-compact-wideart", {**MUSIC, "mediaArtUrl": "@asset:cover-wide.png"}, note="§3.2 PreserveAspectCrop centring")

# 6-9 music expanded
MUSIC_EXP_H = 10 + 176 + 14
expand_music = [hover(6, 19), {"advance": 600}]
fx("music-expanded-playing", MUSIC, expand_music + [{"capture": ""}], MUSIC_EXP_H, note="§10.6")
fx("music-expanded-paused-cjk", {**MUSIC, "mediaPlaying": False, "spectrumValues": [],
                                 "mediaTitle": "夜に駆ける・夜空はいつでも最高密度の青色だ（Long Title Elision Test）",
                                 "mediaArtist": ""},
   expand_music + [{"capture": ""}], MUSIC_EXP_H, note="§10.7 CJK fallback + elision, no artist")
fx("music-expanded-nolength-hover-next", {**MUSIC, "mediaLengthSeconds": 0, "mediaPositionSeconds": 0},
   expand_music + [hover(256, 141), {"advance": 300}, {"capture": ""}], MUSIC_EXP_H,
   note="§10.8 '--:--', no fill, next button hover scale 1.06")
fx("music-expanded-seeking", MUSIC,
   expand_music + [{"press": {"item": "island", "x": 113, "y": 101}},
                   {"drag": {"item": "island", "x": 290, "y": 101}},
                   {"capture": "held"},
                   {"release": True}, {"advance": 200}, {"capture": "released"}],
   MUSIC_EXP_H, note="§10.9 scrub preview (seeking=true, fill width instant)")

# 10 single agent compact
fx("agent-compact-running", {"agentSessions": [RUNNING]}, note="§10.10")
fx("agent-compact-approval", {"agentSessions": [APPROVAL]}, note="§10.10")
fx("agent-compact-completed", {"agentSessions": [DONE]}, note="§10.10 ✓ in ring")
fx("agent-compact-claude", {"agentSessions": [PROCESSING]}, note="§10.10 claude logo")
fx("agent-compact-unknown-source", {"agentSessions": [agent(0, "running", source="gemini", progress=0.4)]},
   note="§10.10 letter fallback 'G'")
fx("agent-compact-progress-zero", {"agentSessions": [agent(0, "running", progress=0)]},
   note="§3.5 zero-progress ring (dot or not)")

# 11 single agent expanded
def agent_expanded(name, sessions, extra_steps=None, note=""):
    h = 10 + expanded_agent_height(len(sessions)) + 14
    fx(name, {"agentSessions": sessions}, [hover(10, 19), {"advance": 600}] + (extra_steps or [{"capture": ""}]), h, note)


mixed = [APPROVAL, QUESTION, RUNNING, PROCESSING, DONE, agent(5, "completed", source="claude", progress=1),
         agent(6, "running", progress=0.3), agent(7, "processing", progress=0.7), agent(8, "running", source="claude", progress=0.5)]
mixed = [dict(s, id=f"s{i}", project=p) for i, (s, p) in enumerate(zip(mixed, ["rshell", "quickshell", "hypr", "notes", "dotfiles", "website", "infra", "paper", "scratch"]))]
agent_expanded("agent-expanded-1", [RUNNING], note="§10.11 n=1 (184 tall)")
agent_expanded("agent-expanded-3", mixed[:3], note="§10.11 n=3")
agent_expanded("agent-expanded-done-row", [DONE, dict(DONE, id="d2", project="hypr")], note="§10.11 accent ✓ slots")
agent_expanded("agent-expanded-9-scrolled", mixed,
               [{"capture": "top"}, {"wheel": {"item": "island", "x": 200, "y": 300, "dy": -240}},
                {"advance": 1500}, {"capture": "scrolled"}],
               note="§10.11 n=9 scroll + indicator")

# 12 dual collapsed
for label, a in (("approval", APPROVAL), ("question", QUESTION), ("done", DONE), ("running", RUNNING)):
    fx(f"dual-collapsed-{label}", {**MUSIC, "agentSessions": [a]}, note="§10.12")

# 13-14 dual transitions
DUAL_H = MUSIC_EXP_H
fx("dual-music-expand", {**MUSIC, "agentSessions": [RUNNING]},
   [hover(118, 19), {"frames": {"name": "expand", "at": [0, 30, 60, 105, 150, 210, 400]}},
    AWAY, {"frames": {"name": "collapse", "at": [0, 60, 105, 150, 210, 229, 231, 400]}}],
   DUAL_H, note="§10.13 incl. musicFrameRelease at 230 ms")
fx("dual-agent-expand", {**MUSIC, "agentSessions": [RUNNING]},
   [hover(12, 19), {"frames": {"name": "expand", "at": [0, 70, 140, 210, 400]}}],
   10 + 184 + 14, note="§10.14")

# 15 single music morph
fx("music-expand-collapse", MUSIC,
   [hover(6, 19), {"frames": {"name": "expand", "at": [0, 30, 70, 105, 140, 210, 400]}},
    AWAY, {"frames": {"name": "collapse", "at": [0, 30, 70, 105, 140, 210, 400]}}],
   MUSIC_EXP_H, note="§10.15")
fx("agent-appear", {}, [{"set": {"agentSessions": [RUNNING]}},
                        {"frames": {"name": "appear", "at": [0, 60, 120, 210, 400]}}],
   note="§8.4 idle → single agent (offset 0 → -2)")
fx("music-appear", {}, [{"set": MUSIC}, {"frames": {"name": "appear", "at": [0, 60, 120, 210, 400]}}],
   note="§8.1 idle → music compact")

# 16 calendar host morph (panel content not rendered: ui-panels)
fx("calendar-host", MUSIC,
   [{"set": {"calendar": True}}, {"frames": {"name": "open", "at": [0, 16, 105, 210, 400]}},
    {"set": {"calendar": False}}, {"frames": {"name": "close", "at": [0, 105, 209, 211, 400]}}],
   10 + 500 + 14, note="§10.16 host frame only; calendar panel content is blank")

# 17 swipe
fx("music-swipe-hide", MUSIC,
   [{"press": {"item": "island", "x": 60, "y": 19}}, {"drag": {"item": "island", "x": 130, "y": 21}},
    {"release": True}, {"frames": {"name": "after", "at": [0, 120, 400]}}],
   note="§10.17 swipe right from the clock. Reference shows NO change: under Qt 6.12 DragHandler resets translation before activeChanged(false), so DynamicIsland.qml:1524-1540 always reads dx = 0 (live bug)")
fx("music-swipe-from-side", MUSIC,
   [{"press": {"item": "island", "x": 100, "y": 19}}, {"drag": {"item": "island", "x": 170, "y": 21}},
    {"release": True}, {"frames": {"name": "after", "at": [0, 120, 400]}}],
   note="§4.3 swipe starting on the right hotspot: the hover expands the island first")


def main() -> None:
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.json"):
        old.unlink()
    for name, data in fixtures.items():
        (out / f"{name}.json").write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{len(fixtures)} fixtures → {out}")


if __name__ == "__main__":
    main()
