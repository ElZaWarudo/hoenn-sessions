"""Fail evidence selection at the actual player/save/descriptor boundaries."""

import json
import sys
import tempfile
import stat
from types import SimpleNamespace
import unittest
from dataclasses import replace
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_region_harness as harness
from live_harness_oracles import OracleFailure, check_projection, read_flash
from live_save_capture import capture_directory
from player_transfer_manifest import parse_schema_payload


class EvidenceBoundaryTests(unittest.TestCase):
    def test_complete_copy_waits_for_bound_client_written_acceptance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / "fixture"
            cache.mkdir()
            body = ("schema=1\nplatform=windows-x86_64\nrelease_id=fixture\n"
                    "sequence=7\nissued_at=1\nexpires_at=2\npayload_sha256=" + "a" * 64 + "\n")
            (cache / ".complete").write_text(body, newline="\n")
            self.assertFalse(harness.accepted_family_ready(cache))
            marker = root / (".accepted-generation-" + "a" * 32)
            marker.write_text(body, newline="\n")
            head = root / (".accepted-head-" + "b" * 32)
            text = f"marker_file={marker.name}\nmarker_sha256={harness.digest(marker)}\n{body}"
            head.write_text(text, newline="\n")
            self.assertTrue(harness.accepted_family_ready(cache))
            for changed, error in ((text + "sequence=7\n", "ambiguous"),
                                   (text.replace(marker.name, "../outside"), "left generation"),
                                   (text.replace("sequence=7", "sequence=8"), "disagree")):
                head.write_text(changed, newline="\n")
                with self.subTest(error=error), self.assertRaisesRegex(harness.HarnessFailure, error):
                    harness.accepted_family_ready(cache)
            head.write_text(text.replace(harness.digest(marker), "0" * 64), newline="\n")
            with self.assertRaisesRegex(harness.HarnessFailure, "hash changed"):
                harness.accepted_family_ready(cache)

    def test_newer_accepted_family_blocks_old_fixture_even_with_valid_old_head(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            body = ("schema=1\nplatform=windows-x86_64\nrelease_id=fixture\nsequence=7\n"
                    "issued_at=1\nexpires_at=2\npayload_sha256=" + "a" * 64 + "\n")
            cache = root / "fixture"
            cache.mkdir()
            (cache / ".complete").write_text(body, newline="\n")
            for index, content in enumerate((body, body.replace("sequence=7", "sequence=8"))):
                marker = root / (".accepted-generation-" + str(index) * 32)
                marker.write_text(content, newline="\n")
                head = root / (".accepted-head-" + str(index) * 32)
                head.write_text(f"marker_file={marker.name}\nmarker_sha256={harness.digest(marker)}\n{content}", newline="\n")
            with self.assertRaisesRegex(harness.HarnessFailure, "another signed family"):
                harness.accepted_family_ready(cache)

    def test_newer_orphan_and_malformed_unreferenced_marker_block_readiness(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            body = ("schema=1\nplatform=windows-x86_64\nrelease_id=fixture\nsequence=7\n"
                    "issued_at=1\nexpires_at=2\npayload_sha256=" + "a" * 64 + "\n")
            cache = root / "fixture"
            cache.mkdir()
            (cache / ".complete").write_text(body, newline="\n")
            marker = root / (".accepted-generation-" + "a" * 32)
            marker.write_text(body, newline="\n")
            (root / (".accepted-head-" + "b" * 32)).write_text(
                f"marker_file={marker.name}\nmarker_sha256={harness.digest(marker)}\n{body}", newline="\n")
            orphan = root / (".accepted-generation-" + "c" * 32)
            orphan.write_text(body.replace("sequence=7", "sequence=8"), newline="\n")
            with self.assertRaisesRegex(harness.HarnessFailure, "orphaned"):
                harness.accepted_family_ready(cache)
            orphan.write_text("unreferenced malformed marker")
            with self.assertRaisesRegex(harness.HarnessFailure, "malformed accepted-generation"):
                harness.accepted_family_ready(cache)

    def test_older_marker_requires_matching_retained_complete_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            body = ("schema=1\nplatform=windows-x86_64\nrelease_id=fixture\nsequence=7\n"
                    "issued_at=1\nexpires_at=2\npayload_sha256=" + "a" * 64 + "\n")
            cache = root / "fixture"
            cache.mkdir()
            (cache / ".complete").write_text(body, newline="\n")
            marker = root / (".accepted-generation-" + "a" * 32)
            marker.write_text(body, newline="\n")
            (root / (".accepted-head-" + "b" * 32)).write_text(
                f"marker_file={marker.name}\nmarker_sha256={harness.digest(marker)}\n{body}", newline="\n")
            older = body.replace("release_id=fixture", "release_id=older").replace("sequence=7", "sequence=6")
            (root / (".accepted-generation-" + "c" * 32)).write_text(older, newline="\n")
            with self.assertRaisesRegex(harness.HarnessFailure, "complete generation is missing"):
                harness.accepted_family_ready(cache)
            generation = root / "older"
            generation.mkdir()
            (generation / ".complete").write_text(body, newline="\n")
            with self.assertRaisesRegex(harness.HarnessFailure, "differs from its complete"):
                harness.accepted_family_ready(cache)
            (generation / ".complete").write_text(older, newline="\n")
            self.assertTrue(harness.accepted_family_ready(cache))

    def test_cache_reparse_path_rejected_before_signature_or_artifact_reads(self):
        metadata = SimpleNamespace(st_mode=stat.S_IFDIR, st_file_attributes=0x400)
        with mock.patch.object(Path, "lstat", return_value=metadata), \
             mock.patch.object(harness, "_paths") as paths:
            with self.assertRaisesRegex(harness.HarnessFailure, "reparse"):
                harness.check_installed_family({}, Path("cache"))
            paths.assert_not_called()

    def test_regular_hardlink_cache_is_allowed_but_canonicalization_failure_is_not(self):
        metadata = SimpleNamespace(st_mode=stat.S_IFREG, st_file_attributes=0, st_nlink=2)
        with mock.patch.object(Path, "lstat", return_value=metadata), \
             mock.patch.object(Path, "resolve", return_value=Path("cache")):
            harness.require_plain_cache_path(Path("cache"))
        if harness.os.name == "nt":
            with mock.patch.object(Path, "lstat", return_value=metadata), \
                 mock.patch.object(Path, "resolve", side_effect=OSError("unmapped volume")):
                with self.assertRaisesRegex(harness.HarnessFailure, "canonicalized"):
                    harness.require_plain_cache_path(Path("cache"))

    def setUp(self):
        self.source = ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav"
        self.descriptor = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()

    def test_world_ready_rejects_menu_state_even_with_active_cloud_session(self):
        status = dict.fromkeys(("initialized", "session_ready", "player_state_sent"), True)
        status.update(dict.fromkeys(("world_not_ready", "queue_error", "checksum_error",
                                    "protocol_error", "sidecar_heartbeat_stale"), False))
        status.update(player_state_sent=False, world_not_ready=True)
        with mock.patch.object(harness, "_capture_bridge_diagnostic", return_value={
                "bridge_candidates": [{"status": status}]}), \
             mock.patch.object(harness, "checkpoint") as record:
            with self.assertRaisesRegex(harness.HarnessFailure, "has not published compatible presence"):
                harness.require_presence_published(1, Path("release"), 1, Path("run"), "cold-load")
            record.assert_not_called()

    def test_world_ready_rejects_ambiguous_bridge(self):
        with mock.patch.object(harness, "_capture_bridge_diagnostic", return_value={
                "bridge_candidates": [{}, {}]}):
            with self.assertRaisesRegex(harness.HarnessFailure, "one live ROM bridge"):
                harness.require_presence_published(1, Path("release"), 1, Path("run"), "cold-load")

    def test_historical_presence_in_menu_claims_only_publication(self):
        # The bridge can publish a hidden pose from its last visible pose in
        # a menu. These flags establish publication, not current controls.
        status = dict.fromkeys(("initialized", "session_ready", "player_state_sent"), True)
        status.update(dict.fromkeys(("world_not_ready", "queue_error", "checksum_error",
                                    "protocol_error", "sidecar_heartbeat_stale"), False))
        with mock.patch.object(harness, "_capture_bridge_diagnostic", return_value={
                "bridge_candidates": [{"status": status}]}), \
             mock.patch.object(harness, "checkpoint") as record:
            harness.require_presence_published(1, Path("release"), 1, Path("run"), "cold-load")
            record.assert_called_once_with(Path("run"), "rom-presence-published",
                                           {"boundary_name": "cold-load", "world_id": 1})

    def test_runtime_stop_uses_owned_desktop_controller_before_child_exit(self):
        plan = {"players": [{"name": "a", "profile_localappdata": "profile"}]}
        child = mock.Mock(pid=100)
        parent = mock.Mock()
        parent.children.side_effect = [[child], []]
        adapter = mock.Mock()
        with mock.patch.object(harness, "Win32Adapter", return_value=adapter), \
             mock.patch.object(harness, "_mgba_pid_for", return_value=100), \
             mock.patch.object(harness.psutil, "Process", return_value=parent), \
             mock.patch.object(harness, "wait_desktop_window"), \
             mock.patch.object(harness, "click_desktop") as click:
            harness.stop_runtime(plan, {"a": 20})
            click.assert_called_once_with(20, 235, 178, adapter)
            adapter.close.assert_not_called()

    def test_verify_revalidates_fixture_before_loading_evidence(self):
        with mock.patch.object(harness, "preflight", side_effect=harness.HarnessFailure("catalog changed")):
            with self.assertRaisesRegex(harness.HarnessFailure, "catalog changed"):
                harness.verify_leg({}, "return", {})

    def test_journey_stops_before_second_leg_after_first_oracle_failure(self):
        import live_group_evidence
        plan = {"players": [{"name": "a", "character_id": "a", "profile_localappdata": "a"},
                            {"name": "b", "character_id": "b", "profile_localappdata": "b"}],
                "legs": [{"name": "outbound", "source_world_id": 1, "destination_world_id": 2,
                          "inputs": [{"player": "a"}], "arrival_inputs": [{"player": "a"}, {"player": "b"}]},
                         {"name": "return", "source_world_id": 2, "destination_world_id": 1,
                          "inputs": [{"player": "b"}], "arrival_inputs": [{"player": "a"}, {"player": "b"}]}]}
        journal = [(1, Path("journal"), {"intent": {"request": {"group_id": "group"}}})]
        adapter = mock.Mock()
        adapter.windows.return_value = []
        with tempfile.TemporaryDirectory() as folder, \
             mock.patch.object(harness, "preflight"), \
             mock.patch.object(harness, "_paths", return_value=(Path(folder), Path(folder))), \
             mock.patch.object(harness, "launch", return_value={"a": 1, "b": 2}), \
             mock.patch.object(harness, "start_games", return_value={"a": 3, "b": 4}) as start, \
             mock.patch.object(harness, "drive") as drive, \
             mock.patch.object(harness, "wait_arrival_games"), \
             mock.patch.object(harness, "continue_arrivals"), \
             mock.patch.object(harness, "_journal_candidates", return_value=journal), \
             mock.patch.object(harness, "stop_runtime"), \
             mock.patch.object(harness, "close_desktops"), \
             mock.patch.object(harness, "Win32Adapter", return_value=adapter), \
             mock.patch.object(live_group_evidence, "collect", return_value={"group": "group.json"}), \
             mock.patch.object(harness, "discover_evidence", return_value={}), \
             mock.patch.object(harness, "verify_leg", side_effect=OracleFailure("Bag changed")):
            with self.assertRaisesRegex(OracleFailure, "Bag changed"):
                harness.journey(plan)
            self.assertEqual(start.call_count, 1)
            self.assertEqual(drive.call_args.args[1], "outbound")
            self.assertEqual(drive.call_count, 1)

    def test_second_client_launch_failure_closes_first_owned_client(self):
        plan = {"players": [{"name": "a", "profile_localappdata": "a"},
                            {"name": "b", "profile_localappdata": "b"}]}
        with tempfile.TemporaryDirectory() as folder, \
             mock.patch.object(harness, "preflight"), \
             mock.patch.object(harness, "_paths", return_value=(Path(folder), Path(folder))), \
             mock.patch.object(harness, "checkpoint_orphans"), \
             mock.patch.object(harness, "launch_signed_desktop", side_effect=[mock.Mock(pid=123), RuntimeError("B failed")]), \
             mock.patch.object(harness, "close_desktops") as close:
            with self.assertRaisesRegex(RuntimeError, "B failed"):
                harness.launch(plan)
            close.assert_called_once_with({"a": 123})

    def test_desktop_cleanup_is_attempted_when_runtime_shutdown_fails(self):
        plan = {"legs": [{"name": "outbound", "inputs": [1], "arrival_inputs": [{"player": "a"}]}],
                "players": [{"name": "a"}]}
        with mock.patch.object(harness, "preflight"), \
             mock.patch.object(harness, "_paths", return_value=(Path("release"), Path("run"))), \
             mock.patch.object(harness, "launch", return_value={"a": 123}), \
             mock.patch.object(harness, "start_games", side_effect=OracleFailure("first boundary")), \
             mock.patch.object(harness, "stop_runtime", side_effect=RuntimeError("drain failed")), \
             mock.patch.object(harness, "close_desktops") as close:
            with self.assertRaisesRegex(OracleFailure, "first boundary") as raised:
                harness.journey(plan)
            close.assert_called_once_with({"a": 123})
            self.assertIn("drain failed", raised.exception.__notes__[0])

    def test_exact_parked_source_is_selected_and_baseline_substitution_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sha = read_flash(self.source).sha256
            record = capture_directory(root, "outbound", "evidence") / "evidence.json"
            record.parent.mkdir(parents=True)
            record.write_text(json.dumps({"players": {"a": {"source": str(self.source),
                                                            "journal": "retired.json",
                                                            "journal_source_sha256": sha}}}))
            receipt = {"boundary": "outbound-verified", "leg": "outbound", "fixture_key": "fixture",
                       "source_world_id": 1, "destination_world_id": 2, "portal_id": "outbound",
                       "players": [{"name": "a", "character_id": "a", "source_sha256": sha,
                                    "journal_source_sha256": sha, "exact_journal_source_inspected": True}],
                       "group": {"members": [{"character_id": "a"}, {"character_id": "b"}]}}
            checkpoint = root / "checkpoint.json"
            checkpoint.write_text(json.dumps({"events": [receipt]}))
            plan = {"players": [{"name": "a", "character_id": "a"}, {"name": "b", "character_id": "b"}],
                    "legs": [{"name": "outbound", "source_world_id": 1, "destination_world_id": 2,
                              "portal_id": "outbound"}]}
            leg = {"destination_base_leg": "outbound", "source_world_id": 2, "destination_world_id": 1}
            with mock.patch.object(harness, "_paths", return_value=(root, root)), \
                 mock.patch.object(harness, "_fixture_key", return_value="fixture"):
                path, kind, actual = harness._destination_base(plan, leg, {"name": "a", "character_id": "a"}, root)
                self.assertEqual((path, kind, actual), (self.source, "dormant", sha))
                swapped = dict(plan, players=[{"name": "a", "character_id": "b"},
                                               {"name": "b", "character_id": "a"}])
                with self.assertRaisesRegex(harness.HarnessFailure, "another character"):
                    harness._destination_base(swapped, leg, swapped["players"][0], root)
                for field, value in (("source_world_id", 3), ("destination_world_id", 4),
                                     ("fixture_key", "other"), ("portal_id", "other")):
                    changed = dict(receipt, **{field: value})
                    checkpoint.write_text(json.dumps({"events": [changed]}))
                    with self.assertRaisesRegex(harness.HarnessFailure, "another fixture or world pair"):
                        harness._destination_base(plan, leg, plan["players"][0], root)
                for field, value in (("exact_journal_source_inspected", False),
                                     ("source_sha256", "0" * 64), ("journal_source_sha256", "0" * 64)):
                    changed = json.loads(json.dumps(receipt))
                    changed["players"][0][field] = value
                    checkpoint.write_text(json.dumps({"events": [changed]}))
                    with self.assertRaises(harness.HarnessFailure):
                        harness._destination_base(plan, leg, {"name": "a", "character_id": "a"}, root)
                receipt["group"]["members"][0]["character_id"] = "other"
                checkpoint.write_text(json.dumps({"events": [receipt]}))
                with self.assertRaises(harness.HarnessFailure):
                    harness._destination_base(plan, leg, {"name": "a", "character_id": "a"}, root)
                checkpoint.write_text(json.dumps({"events": []}))
                with self.assertRaisesRegex(harness.HarnessFailure, "no verified preceding leg"):
                    harness._destination_base(plan, leg, {"name": "a", "character_id": "a"}, root)

    def test_retained_journal_survives_retirement_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            live = root / "live.json"
            journal = {"character_id": "a", "phase": "adopted"}
            live.write_text(json.dumps(journal))
            item = harness._retain_journal(live, journal, root / "captured")
            self.assertEqual(harness._retain_journal(live, journal, root / "captured"), item)
            live.unlink()
            self.assertEqual(harness._read_evidence_journal(item, "retained"), journal)
            Path(item["journal"]).write_text(json.dumps(dict(journal, character_id="b")))
            with self.assertRaisesRegex(harness.HarnessFailure, "missing or changed"):
                harness._read_evidence_journal(item, "retained")

    def test_identical_captured_copy_precedes_volatile_copy(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            captured, volatile = root / "captured", root / "volatile"
            captured.mkdir()
            volatile.mkdir()
            for location in (captured, volatile):
                (location / "same.sav").write_bytes(self.source.read_bytes())
            sha = read_flash(self.source).sha256
            matches = harness._save_candidates([captured, volatile], sha)
            self.assertEqual(matches, [captured / "same.sav", volatile / "same.sav"])

    def test_projection_rejects_shared_world_local_and_coop_corruption(self):
        source = read_flash(self.source)
        check_projection(source, source, source, self.descriptor, expected_generation_delta=0)
        fields = parse_schema_payload(self.descriptor)["fields"]
        # Change one byte through the oracle's field API; ROM validation is
        # tested separately. This isolates ownership and payload comparisons.
        for owner, field in (("shared player", next(f for f in fields if f["id"] == 0x0105)),
                             ("world-local", next(f for f in fields if f["ownership"] == 2
                                                  and f["id"] != 0x020B))):
            original = source.field
            def changed(storage, offset, size):
                value = original(storage, offset, size)
                if (storage, offset, size) == (field["storage"], field["offset"], field["size"]):
                    return bytes([value[0] ^ 1]) + value[1:]
                return value
            destination = mock.Mock(wraps=source)
            destination.lineage, destination.generation, destination.coop = source.lineage, source.generation, source.coop
            destination.sha256 = source.sha256
            destination.field = changed
            with self.assertRaisesRegex(OracleFailure, owner):
                check_projection(source, destination, source, self.descriptor, expected_generation_delta=0)
        coop = bytearray(source.coop)
        coop[100] ^= 1
        with self.assertRaisesRegex(OracleFailure, "membership/progress"):
            check_projection(source, replace(source, coop=bytes(coop)), source,
                             self.descriptor, expected_generation_delta=0)


if __name__ == "__main__":
    unittest.main()

