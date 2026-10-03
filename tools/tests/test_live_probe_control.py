"""Filesystem producer tests only; no emulator or server is launched."""
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_probe_control as control
from live_harness_oracles import OracleFailure


class ProbeControlTests(unittest.TestCase):
    def request(self):
        return {"label": "third-mail", "actions": [{"mask": 128}, {"mask": 1, "hold": 8, "wait": 600}],
                "checkpoint": True}

    def test_complete_json_visible_only_after_fsync(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            real_fsync = os.fsync
            def sync(fd):
                self.assertFalse((root / "control-0001.json").exists())
                self.assertEqual(json.loads((root / "control-0001.tmp").read_text()),
                                 control.validate_control(1, self.request()))
                real_fsync(fd)
            with mock.patch.object(control.os, "fsync", side_effect=sync) as called:
                result = control.publish_control(root, 1, self.request())
            called.assert_called_once()
            self.assertEqual(json.loads(result.read_text()), control.validate_control(1, self.request()))
            self.assertEqual(list(root.iterdir()), [result])

    def test_duplicate_and_existing_pending_no_overwrite(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for suffix in ("json", "tmp"):
                existing = root / f"control-0001.{suffix}"
                existing.write_bytes(b"retained")
                with self.assertRaises(OracleFailure): control.publish_control(root, 1, self.request())
                self.assertEqual(existing.read_bytes(), b"retained")
                self.assertEqual(list(root.iterdir()), [existing])
                existing.unlink()

    def test_malformed_requests_create_no_files(self):
        invalid = [None, [], {}, {"done": False}, {"done": 1}, {"done": None},
                   {"done": True, "label": "done"}, {"done": True, "actions": []},
                   {"label": "x", "actions": [], "checkpoint": False},
                   {"label": "x", "actions": [{}] * 121},
                   {"label": "x", "actions": [{}], "checkpoint": 1},
                   {"label": "x", "actions": [{}], "checkpoint": None},
                   {"label": "x", "actions": [{}], "unknown": True},
                   {"label": "../x", "actions": [{}]}, {"label": "é", "actions": [{}]},
                   {"label": "x" * 49, "actions": [{}]},
                   {"label": "x" * 48, "actions": [{}, {}]},
                   {"label": "x", "actions": [None]}, {"label": "x", "actions": [{"extra": 0}]}]
        for field, values in (("mask", [-1, 1024, True, "1"]), ("hold", [0, 121, True, 1.5]),
                              ("wait", [-1, 7201, False, None])):
            invalid += [{"label": "x", "actions": [{field: value}]} for value in values]
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for request in invalid:
                with self.subTest(request=request), self.assertRaises(OracleFailure):
                    control.publish_control(root, 1, request)
                self.assertEqual(list(root.iterdir()), [])
            for sequence in (0, 1001, -1, True, 1.5, "1"):
                with self.subTest(sequence=sequence), self.assertRaises(OracleFailure):
                    control.publish_control(root, sequence, self.request())
                self.assertEqual(list(root.iterdir()), [])

    def test_directory_required_and_bounds_valid(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            missing = root / "missing"
            with self.assertRaises(OracleFailure): control.publish_done(missing, 1)
            self.assertFalse(missing.exists())
            request = {"label": "edge", "actions": [{"mask": 1023, "hold": 120, "wait": 7200}] * 120}
            result = control.publish_control(root, 1000, request)
            self.assertEqual(len(json.loads(result.read_text())["actions"]), 120)

    def test_failed_fsync_cleans_owned_pending(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with mock.patch.object(control.os, "fsync", side_effect=OSError("unit disk failure")):
                with self.assertRaises(OSError): control.publish_control(root, 1, self.request())
            self.assertEqual(list(root.iterdir()), [])

    def test_destination_race_keeps_other_writer_and_cleans_pending(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            destination = root / "control-0001.json"
            real_fsync = os.fsync
            def sync(fd):
                real_fsync(fd)
                destination.write_bytes(b"other-writer-retained")
            with mock.patch.object(control.os, "fsync", side_effect=sync):
                with self.assertRaises(FileExistsError):
                    control.publish_control(root, 1, self.request())
            self.assertEqual(destination.read_bytes(), b"other-writer-retained")
            self.assertEqual(list(root.iterdir()), [destination])

    def test_done_and_cli(self):
        with tempfile.TemporaryDirectory() as temp, redirect_stdout(io.StringIO()):
            root = Path(temp)
            self.assertEqual(control.main(["--root", temp, "--sequence", "1", "--label", "checkpoint",
                "--actions-json", '[{"mask":0,"hold":1,"wait":300}]', "--checkpoint"]), 0)
            self.assertTrue(json.loads((root / "control-0001.json").read_text())["checkpoint"])
            self.assertEqual(control.main(["--root", temp, "--sequence", "2", "--done"]), 0)
            self.assertEqual(json.loads((root / "control-0002.json").read_text()), {"done": True})
            with redirect_stdout(io.StringIO()), mock.patch("sys.stderr", new_callable=io.StringIO):
                for args in (["--done", "--checkpoint"], ["--done", "--actions-json", "[]"],
                             ["--label", "x"], ["--label", "x", "--actions-json", "{"]):
                    with self.assertRaises(SystemExit):
                        control.main(["--root", temp, "--sequence", "3"] + args)
            self.assertFalse((root / "control-0003.json").exists())


if __name__ == "__main__":
    unittest.main()
