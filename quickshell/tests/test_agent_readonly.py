"""Run with python3 -m unittest discover -s quickshell/tests -v."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
import agent_island


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


hook = load_script("agent-island-hook")
action = load_script("agent-island-action")
status = load_script("agent-island-status")


class ReadOnlyAgentTests(unittest.TestCase):
    def test_permission_and_question_hooks_return_without_a_decision(self):
        cases = [
            ("PermissionRequest", {"tool_name": "Bash", "tool_input": {"command": "true"}}, "waitingApproval"),
            ("PermissionRequest", {"tool_name": "AskUserQuestion", "tool_input": {
                "questions": [{"question": "Which?", "options": [{"label": "One"}]}]
            }}, "waitingQuestion"),
            ("Notification", {"question": "Which?", "options": ["One"]}, "waitingQuestion"),
        ]
        for event, fields, expected in cases:
            with self.subTest(event=event, fields=fields), tempfile.TemporaryDirectory() as directory:
                payload = {"session_id": "test-session", "hook_event_name": event, **fields}
                stdin = io.TextIOWrapper(io.BytesIO(json.dumps(payload).encode()))
                stdout = io.StringIO()
                with patch.dict(os.environ, {"HYPRV_AGENT_ISLAND_DIR": directory}), \
                        patch.object(sys, "argv", ["hook", "--source", "claude"]), \
                        patch.object(sys, "stdin", stdin), contextlib.redirect_stdout(stdout), \
                        patch.object(agent_island, "terminal_meta", return_value={}):
                    self.assertEqual(hook.main(), 0)
                    state = agent_island.read_state()
                self.assertEqual(stdout.getvalue(), "")
                self.assertEqual(state["sessions"]["test-session"]["status"], expected)
                self.assertNotIn("pending_request_id", state["sessions"]["test-session"])
                self.assertEqual(state["pending_requests"], {})
                self.assertEqual(list((Path(directory) / "responses").iterdir()), [])

    def test_focus_remains_available_without_writing_state(self):
        session = {"id": "task", "title": "Example"}
        with patch.object(sys, "argv", ["action", "focus", "task"]), \
                patch.object(action, "locked_state") as locked, \
                patch.object(action, "focus_session", return_value=0) as focus:
            locked.return_value.__enter__.return_value = {"sessions": {"task": session}}
            self.assertEqual(action.main(), 0)
            locked.assert_called_once_with(write=False)
            focus.assert_called_once_with(session)

    def test_response_actions_are_removed(self):
        for operation in ("approve", "deny", "answer", "reply"):
            with self.subTest(operation=operation), \
                    patch.object(sys, "argv", ["action", operation, "task"]), \
                    contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as exited:
                action.main()
            self.assertEqual(exited.exception.code, 2)

    def test_watch_suppresses_unchanged_tasks_but_emits_changes(self):
        snapshots = [
            {"active": False, "count": 0, "sessions": [], "updated_at": 1},
            {"active": False, "count": 0, "sessions": [], "updated_at": 2},
            {"active": True, "count": 1, "sessions": [{"id": "new"}], "updated_at": 3},
        ]
        stdout = io.StringIO()
        with patch.object(sys, "argv", ["status", "--json", "--watch"]), \
                patch.object(status, "locked_state") as locked, \
                patch.object(status, "public_snapshot", side_effect=snapshots), \
                patch.object(status.time, "sleep", side_effect=[None, None, InterruptedError]) as sleep, \
                contextlib.redirect_stdout(stdout), self.assertRaises(InterruptedError):
            locked.return_value.__enter__.return_value = {}
            status.main()
        lines = [json.loads(line) for line in stdout.getvalue().splitlines()]
        self.assertEqual([line["count"] for line in lines], [0, 1])
        self.assertEqual([call.args[0] for call in sleep.call_args_list], [1.2, 1.2, 0.65])


if __name__ == "__main__":
    unittest.main()
