"""Validate evidence timing using real ROM-written saves, without an emulator."""

import sys
import os
import json
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))

from live_harness_oracles import read_flash, read_flash_bytes
from live_save_capture import SaveCapture, capture_directory


class SaveCaptureTests(unittest.TestCase):
    def setUp(self):
        self.fixture = ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav"

    def test_keeps_exact_checkpoint_after_client_retires_source_during_wait(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            session = root / "sessions/coop-session-one"
            session.mkdir(parents=True)
            source = session / "character.sav"
            output = root / "captured"
            data = self.fixture.read_bytes()
            sha = read_flash_bytes(data, source).sha256
            with SaveCapture(root / "sessions", output, interval=.005):
                source.write_bytes(data)
                deadline = time.monotonic() + 2
                while not (output / (sha + ".sav")).exists():
                    if time.monotonic() >= deadline:
                        self.fail("capture did not observe source during blocked action")
                    time.sleep(.01)
                source.unlink()  # Simulate launcher cleanup before receipt.
                time.sleep(.02)
            self.assertEqual((output / (sha + ".sav")).read_bytes(), data)
            self.assertEqual(read_flash(output / (sha + ".sav")).sha256, sha)

    def test_skips_partial_write_then_captures_valid_initial_head(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            session = root / "sessions/coop-session-one"
            session.mkdir(parents=True)
            source = session / "character.sav"
            source.write_bytes(b"partial")
            capture = SaveCapture(root / "sessions", root / "out")
            capture.scan()
            self.assertEqual(list((root / "out").glob("*.sav")), [])
            source.write_bytes(self.fixture.read_bytes())
            with capture:
                self.assertEqual(len(list((root / "out").glob("*.sav"))), 1)

    def test_never_overwrites_changed_digest_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            session = root / "sessions/coop-session-one"
            session.mkdir(parents=True)
            source = session / "character.sav"
            source.write_bytes(self.fixture.read_bytes())
            capture = SaveCapture(root / "sessions", root / "out")
            capture.scan()
            next((root / "out").glob("*.sav")).write_bytes(b"changed")
            with self.assertRaisesRegex(RuntimeError, "digest file changed"):
                capture.scan()

    def test_path_labels_cannot_escape_or_collide_by_windows_casing(self):
        root = Path("run")
        a = capture_directory(root, "../../outbound", "A")
        b = capture_directory(root, "../../outbound", "a")
        self.assertEqual(a.parent.name, "captured-saves")
        self.assertEqual(a.parent.parent.name, "run")
        self.assertNotEqual(a, b)

    @unittest.skipUnless(os.name == "nt", "Windows MAX_PATH regression")
    def test_nested_capture_publishes_and_consumer_reads_beyond_max_path(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            session = root / "sessions/coop-session-one"
            session.mkdir(parents=True)
            source = session / "character.sav"
            data = self.fixture.read_bytes()
            source.write_bytes(data)
            # The real failure had two 64-character keys and a digest filename.
            run = root / ("r" * 64) / "journey"
            output = capture_directory(run, "outbound", "a")
            sha = read_flash_bytes(data, source).sha256
            target = output / (sha + ".sav")
            self.assertGreater(len(str(target).removeprefix("\\\\?\\")), 260)
            capture = SaveCapture(root / "sessions", output)
            capture.scan()
            # Reconstruct through the public consumer API, rather than reusing
            # the writer's output path; discovery and receipt checks do this.
            consumer = capture_directory(run, "outbound", "a") / (sha + ".sav")
            self.assertEqual(consumer.read_bytes(), data)
            self.assertEqual(read_flash(consumer).sha256, sha)
            serialized = Path(json.loads(json.dumps({"source": str(consumer)}))["source"])
            self.assertEqual(serialized.read_bytes(), data)
            self.assertEqual(list(output.rglob("*.sav")), [serialized])
            self.assertEqual(list(output.glob("*.tmp")), [])
            self.assertEqual(list(output.glob("*.sav")), [consumer])
            capture.scan()  # Existing immutable evidence is still checked.
            self.assertEqual(source.read_bytes(), data)
            consumer.write_bytes(b"changed")
            with self.assertRaisesRegex(RuntimeError, "digest file changed"):
                capture.scan()
            # Ordinary tempfile cleanup has the same MAX_PATH limitation.
            consumer.unlink()

    @unittest.skipUnless(os.name == "nt", "Windows extended path regression")
    def test_direct_output_path_is_extended_before_publication(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            session = root / "sessions/coop-session-one"
            session.mkdir(parents=True)
            data = self.fixture.read_bytes()
            (session / "character.sav").write_bytes(data)
            output = root / ("r" * 64) / ("a" * 64) / ("c" * 20)
            capture = SaveCapture(root / "sessions", output)
            capture.scan()
            self.assertTrue(str(capture.output).startswith("\\\\?\\"))
            target = next(capture.output.glob("*.sav"))
            self.assertGreater(len(str(target).removeprefix("\\\\?\\")), 260)
            self.assertEqual(target.read_bytes(), data)
            target.unlink()

    @unittest.skipUnless(os.name == "nt", "Windows UNC path normalization")
    def test_unc_capture_path_keeps_share_and_is_idempotent(self):
        directory = capture_directory(Path(r"\\server\share\run"), "leg", "a")
        self.assertTrue(str(directory).startswith(r"\\?\UNC\server\share\run\captured-saves"))
        capture = SaveCapture(Path("sessions"), directory)
        self.assertEqual(capture.output, directory)

    def test_image_limit_stops_before_growing_output(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            session = root / "sessions/coop-session-one"
            session.mkdir(parents=True)
            source = session / "character.sav"
            source.write_bytes(self.fixture.read_bytes())
            capture = SaveCapture(root / "sessions", root / "out", limit=1)
            capture.scan()
            source.write_bytes((ROOT / "tools/tests/fixtures/arrival-v3-cormoria-rivetshore.sav").read_bytes())
            with self.assertRaisesRegex(RuntimeError, "image limit"):
                capture.scan()
            self.assertEqual(len(list((root / "out").glob("*.sav"))), 1)


if __name__ == "__main__":
    unittest.main()
