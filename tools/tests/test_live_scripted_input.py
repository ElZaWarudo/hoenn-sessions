import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_scripted_input as scripted
from live_harness_oracles import OracleFailure


class ScriptedInputTests(unittest.TestCase):
    def test_request_rejects_injection_unbounded_or_noninteger_controls(self):
        self.assertEqual(scripted.request_line(1, "harbor", 264, 8, 30), "1,264,8,30,harbor\n")
        for args in ((1, "../save", 1, 8, 1), (1, 'x\n1,1,1,1,x', 1, 8, 1),
                     (1, "x", 1024, 8, 1), (1, "x", True, 8, 1),
                     (1, "x", 1, 0, 1), (1, "x", 1, 121, 1),
                     (1, "x", 1, 8, 7201), (1001, "x", 1, 8, 1)):
            with self.subTest(args=args), self.assertRaises(OracleFailure):
                scripted.request_line(*args)

    def test_exact_completion_waits_for_owned_process_and_capture(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def complete():
                self.assertEqual((root / "request-0001.txt").read_text(), "1,1,8,20,harbor\n")
                self.assertFalse((root / "request-0001.tmp").exists())
                (root / "harbor.png").write_bytes(b"\x89PNG\r\n\x1a\n")
                (root / "script-input.jsonl").write_text(json.dumps(
                    {"status": "complete", "sequence": 1, "label": "harbor", "frame": 34}) + "\n")
                return None
            process = mock.Mock()
            process.poll.side_effect = complete
            adapter = scripted.ScriptedInput(root, process)
            self.assertEqual(adapter.act("harbor", 1), root / "harbor.png")
            with self.assertRaises(OracleFailure):
                adapter.act("harbor", 1)

    def test_wrong_label_failure_and_exited_process_stop_at_boundary(self):
        for status in ("wrong_label", "failed", "exited"):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                def fail():
                    (root / "script-input.jsonl").write_text(json.dumps(
                        {"status": "failed" if status == "failed" else "complete",
                         "sequence": 1, "label": "other", "frame": 1}) + "\n")
                    return 1 if status == "exited" else None
                process = mock.Mock()
                process.poll.side_effect = fail
                with self.subTest(status=status), self.assertRaises(OracleFailure):
                    scripted.ScriptedInput(root, process).act("harbor", 1)

    def test_stale_protocol_prevents_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "script-input.jsonl").write_text("old")
            with mock.patch.object(scripted.subprocess, "Popen") as spawn:
                with self.assertRaises(OracleFailure):
                    with scripted.owned_scripted_emulator(Path("mgba.exe"), root, {}):
                        pass
                spawn.assert_not_called()

    def test_incomplete_receipt_reaches_bounded_action_timeout(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "script-input.jsonl").write_text('{"status":"complete"')
            process = mock.Mock()
            process.poll.return_value = None
            with mock.patch.object(scripted.time, "monotonic", side_effect=[0, 0, 2]), \
                    mock.patch.object(scripted.time, "sleep"):
                with self.assertRaisesRegex(OracleFailure, "timed out at harbor"):
                    scripted.ScriptedInput(root, process).act("harbor", timeout=1)

    def test_cleanup_failure_keeps_original_boundary_exception_primary(self):
        with tempfile.TemporaryDirectory() as tmp:
            process = mock.Mock()
            process.terminate.side_effect = OSError("cleanup")
            with mock.patch.object(scripted.subprocess, "Popen", return_value=process):
                with self.assertRaisesRegex(RuntimeError, "boundary") as caught:
                    with scripted.owned_scripted_emulator(Path("mgba.exe"), Path(tmp), {}):
                        raise RuntimeError("boundary")
            self.assertEqual(caught.exception.__notes__, ["owned scripted emulator cleanup failed: cleanup"])

    def test_failure_closes_only_owned_emulator_and_kills_after_deadline(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            process = mock.Mock()
            process.wait.side_effect = [subprocess.TimeoutExpired("mgba", 10), 0]
            with mock.patch.object(scripted.subprocess, "Popen", return_value=process) as spawn:
                with self.assertRaisesRegex(RuntimeError, "boundary"):
                    with scripted.owned_scripted_emulator(Path("signed/mgba.exe"), root, {"x": "y"}):
                        raise RuntimeError("boundary")
            self.assertEqual(spawn.call_args.args[0], [str(Path("signed/mgba.exe")), "--script", str(root / "script-input.lua"), str(root / "game.gba")])
            process.terminate.assert_called_once()
            process.kill.assert_called_once()
            self.assertEqual(process.wait.call_args_list, [mock.call(timeout=10), mock.call(timeout=10)])


if __name__ == "__main__":
    unittest.main()
