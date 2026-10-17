import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
import agent_island
spec = importlib.util.spec_from_file_location("desktop_state", SCRIPTS / "desktop-state.py")
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)


class DesktopStateTests(unittest.TestCase):
    def test_stream_only_publishes_changed_values(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            publisher = desktop.Publisher()
            publisher.emit("wifi", {"enabled": True})
            publisher.emit("wifi", {"enabled": True})
            publisher.emit("wifi", {"enabled": False})
        self.assertEqual([json.loads(line)["data"]["enabled"] for line in output.getvalue().splitlines()], [True, False])

    def test_agent_cache_observes_hook_writes_without_overwriting_them(self):
        def snapshot(state, **kwargs):
            state["last_process_scan"] = time.time()
            sessions = list(state["sessions"].values())
            return {"sessions": sessions, "active": bool(sessions), "count": len(sessions)}

        with tempfile.TemporaryDirectory() as directory, \
                patch.dict(os.environ, {"HYPRV_AGENT_ISLAND_DIR": directory}), \
                patch.object(desktop.GLib, "timeout_add"), \
                patch.object(desktop, "public_snapshot", side_effect=snapshot) as observe, \
                contextlib.redirect_stdout(io.StringIO()):
            publisher = desktop.Publisher()
            observer = desktop.AgentObserver(publisher)
            committed = agent_island.state_file().read_bytes()
            observer.tick()
            self.assertEqual(agent_island.state_file().read_bytes(), committed)
            self.assertEqual(observe.call_args.kwargs, {"scan_processes": False})
            with agent_island.locked_state() as state:
                state["sessions"]["new"] = {"id": "new", "status": "running", "terminal": {"pid": os.getpid()}, "tool_history": ["large"]}
            observer.tick()
            self.assertEqual(json.loads(publisher.previous["agent"])["sessions"], [{"id": "new", "status": "running"}])
            self.assertIn("new", agent_island.read_state()["sessions"])
            self.assertNotIn("scan_processes", observe.call_args.kwargs)

    def test_candidates_use_argv_only_for_agents_and_keep_ancestry(self):
        fake_stat = "1 (name) S " + " ".join(["0"] * 19)
        with patch.object(agent_island.os, "listdir", return_value=["123", "456"]), \
                patch("builtins.open", side_effect=[io.StringIO(fake_stat.replace("(name)", "(browser)")), io.StringIO(fake_stat.replace("(name)", "(codex)"))]), \
                patch.object(agent_island, "proc_cmdline", return_value=["/usr/bin/codex"]) as argv:
            processes = agent_island.list_processes()
        self.assertEqual(set(processes), {123, 456})
        self.assertEqual(processes[123]["cmdline"], [])
        argv.assert_called_once_with(456)


if __name__ == "__main__":
    unittest.main()
