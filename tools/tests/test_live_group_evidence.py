"""Collector output admission only; no server or runtime actions."""
import os
from pathlib import Path, PurePosixPath
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_group_evidence as evidence


class EvidenceContainmentTests(unittest.TestCase):
    def test_empty_drive_platform_keeps_containment_without_system_drive_false_positive(self):
        with mock.patch.object(evidence, "_capture_path", side_effect=[PurePosixPath("/spare/run/evidence"),
                                                                      PurePosixPath("/spare/run"), PurePosixPath("/C")]):
            self.assertEqual(evidence._evidence_output(Path("inside"), Path("run")), PurePosixPath("/spare/run/evidence"))
        with mock.patch.object(evidence, "_capture_path", side_effect=[PurePosixPath("/spare/outside/evidence"),
                                                                      PurePosixPath("/spare/run"), PurePosixPath("/C")]):
            with self.assertRaises(evidence.HarnessFailure):
                evidence._evidence_output(Path("outside"), Path("run"))

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_unc_namespace_aliases_match_without_network_io(self):
        run = Path(r"\\server\share\run")
        child = Path(r"\\?\UNC\server\share\run\evidence")
        with mock.patch.object(Path, "resolve", lambda path: path):
            self.assertEqual(evidence._evidence_output(child, run), child)

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_resolved_symlink_escape_is_rejected(self):
        run = Path(r"S:\run")
        child = run / "linked" / "evidence"
        with mock.patch.object(Path, "resolve", lambda path: Path(r"S:\outside\evidence") if path == child else path):
            with self.assertRaises(evidence.HarnessFailure):
                evidence._evidence_output(child, run)

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_mixed_namespaces_accept_inside_preserving_extended_io(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build") as folder:
            run = Path(folder).resolve()
            child = run / "captured-saves" / "actor" / "evidence"
            extended = Path("\\\\?\\" + str(child))
            self.assertEqual(evidence._evidence_output(extended, run), extended)
            self.assertEqual(evidence._evidence_output(child, Path("\\\\?\\" + str(run))), extended)
            self.assertFalse(child.exists())

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_outside_and_c_output_rejected_before_api_or_directory_creation(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build") as folder:
            run = Path(folder).resolve()
            for output, root in ((run.parent / (run.name + "-outside") / "evidence", run),
                                 (Path(r"\\?\C:\cormoria-no-write\evidence"), Path(r"C:\cormoria-no-write")),
                                 (Path(r"C:\cormoria-no-write\evidence"), Path(r"\\?\C:\cormoria-no-write"))):
                with self.subTest(output=output), \
                        mock.patch.object(evidence, "_leg", return_value={}), \
                        mock.patch.object(evidence, "_paths", return_value=(run / "release", root)), \
                        mock.patch.object(evidence, "_call") as api, \
                        mock.patch.object(evidence, "_health_url") as health:
                    with self.assertRaisesRegex(evidence.HarnessFailure, "spare-volume"):
                        evidence.collect({}, "leg", "group", output)
                    api.assert_not_called()
                    health.assert_not_called()
                    self.assertFalse(output.exists())

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_valid_mixed_namespace_collector_reaches_next_gate_without_api(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build") as folder:
            run = Path(folder).resolve()
            output = Path("\\\\?\\" + str(run / "captured-saves" / "evidence"))
            with mock.patch.object(evidence, "_leg", return_value={}), \
                    mock.patch.object(evidence, "_paths", return_value=(run / "release", run)), \
                    mock.patch.object(evidence, "_call") as api, \
                    mock.patch.object(evidence, "_health_url", side_effect=RuntimeError("next gate reached")):
                with self.assertRaisesRegex(RuntimeError, "next gate reached"):
                    evidence.collect({}, "leg", "group", output)
                api.assert_not_called()
                self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
