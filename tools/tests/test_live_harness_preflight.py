"""Offline capacity admission retains real signature and artifact hash checks."""
import base64
from contextlib import ExitStack
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_region_harness as harness


class PreflightSpaceTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.release = self.root / "release"
        self.release.mkdir()
        artifact = self.release / "app/coop-launcher.exe"
        artifact.parent.mkdir()
        artifact.write_bytes(b"unit-only signed artifact")
        key = Ed25519PrivateKey.generate()
        payload = json.dumps({"release_id": "test", "artifacts": [
            {"id": "desktop-app", "size": artifact.stat().st_size, "sha256": harness.digest(artifact)}]}).encode()
        self.envelope = self.release / "release-envelope.json"
        self.envelope.write_text(json.dumps({"payload": base64.b64encode(payload).decode(),
                                            "signature": base64.b64encode(key.sign(payload)).decode()}))
        self.catalog = self.release / "release_catalog.json"
        self.catalog.write_text("{}")
        self.plan = {"release_id": "test", "release_public_key_hex": key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw).hex(),
                     "envelope_sha256": harness.digest(self.envelope), "catalog_sha256": harness.digest(self.catalog)}
        stack = ExitStack()
        self.addCleanup(stack.close)
        # Isolate the capacity/trust boundary, retaining genuine verification and disk-space logic.
        stack.enter_context(mock.patch.object(harness, "_validate_plan_shape"))
        stack.enter_context(mock.patch.object(harness, "_paths", return_value=(self.release, self.root / "run")))
        stack.enter_context(mock.patch.object(harness.shutil, "disk_usage", return_value=SimpleNamespace(free=1)))

    def test_default_floor_rejects_before_signed_client_launch(self):
        with mock.patch.object(harness, "launch_signed_desktop") as launch, \
                mock.patch.object(harness, "_read_json", wraps=harness._read_json) as read:
            with self.assertRaisesRegex(harness.HarnessFailure, "below 2 GiB"):
                harness.launch(self.plan)
            launch.assert_not_called()
            read.assert_not_called()

    def test_offline_skips_only_capacity_and_rejects_invalid_signature(self):
        envelope = json.loads(self.envelope.read_text())
        envelope["signature"] = base64.b64encode(bytes(64)).decode()
        self.envelope.write_text(json.dumps(envelope))
        with mock.patch.object(harness, "check_c_space", wraps=harness.check_c_space) as space:
            with self.assertRaisesRegex(harness.HarnessFailure, "signature or payload"):
                harness.preflight(self.plan, require_live_space=False)
            space.assert_not_called()
        with self.assertRaisesRegex(harness.HarnessFailure, "signature or payload"):
            harness.verify_leg(self.plan, "out", {}, require_live_space=False)
        with self.assertRaisesRegex(harness.HarnessFailure, "signature or payload"):
            harness.discover_evidence(self.plan, "out", require_live_space=False)

    def test_implicit_verify_leg_retains_actual_live_floor(self):
        with self.assertRaisesRegex(harness.HarnessFailure, "below 2 GiB"):
            harness.verify_leg(self.plan, "out", {})

    def test_implicit_discover_evidence_retains_actual_live_floor(self):
        with self.assertRaisesRegex(harness.HarnessFailure, "below 2 GiB"):
            harness.discover_evidence(self.plan, "out")

    def test_offline_still_rejects_signed_artifact_hash_tamper(self):
        artifact = self.release / "app/coop-launcher.exe"
        data = bytearray(artifact.read_bytes()); data[0] ^= 1; artifact.write_bytes(data)
        with self.assertRaisesRegex(harness.HarnessFailure, "signed artifact desktop-app: missing or changed"):
            harness.preflight(self.plan, require_live_space=False)

    def test_offline_still_rejects_catalog_hash_tamper(self):
        self.catalog.write_text('{"tampered":true}')
        with self.assertRaisesRegex(harness.HarnessFailure, "release catalog: missing or changed"):
            harness.preflight(self.plan, require_live_space=False)

    def test_space_option_requires_explicit_boolean(self):
        for value in (None, 0, 1, "false"):
            with self.subTest(value=value), self.assertRaisesRegex(harness.HarnessFailure, "boolean"):
                harness.preflight(self.plan, require_live_space=value)

    def test_verify_leg_forwards_offline_capacity_without_skipping_preflight(self):
        with mock.patch.object(harness, "preflight", side_effect=RuntimeError("trust gate reached")) as preflight:
            with self.assertRaisesRegex(RuntimeError, "trust gate reached"):
                harness.verify_leg(self.plan, "out", {}, require_live_space=False)
            preflight.assert_called_once_with(self.plan, require_live_space=False)
        with mock.patch.object(harness, "preflight", side_effect=RuntimeError("live gate reached")) as preflight:
            with self.assertRaisesRegex(RuntimeError, "live gate reached"):
                harness.verify_leg(self.plan, "out", {})
            preflight.assert_called_once_with(self.plan, require_live_space=True)


if __name__ == "__main__":
    unittest.main()
