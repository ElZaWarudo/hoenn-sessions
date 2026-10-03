"""Focused checks for live save inspection without a running emulator."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
import json
import hashlib
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "coop"))

import live_region_harness as journey
from live_harness_oracles import (OracleFailure, _logical_value, read_flash,
                                 check_shared_witnesses, check_travel_witnesses, logical_field)
from player_transfer_manifest import parse_schema_payload


class LiveOracleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.main = ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav"
        self.cormoria = ROOT / "tools/tests/fixtures/arrival-v3-cormoria-rivetshore.sav"

    def test_reads_both_rom_written_arrival_saves(self) -> None:
        main = read_flash(self.main)
        cormoria = read_flash(self.cormoria)
        self.assertEqual((main.counter, main.generation), (6, 6))
        self.assertEqual((cormoria.counter, cormoria.generation), (6, 6))
        self.assertNotEqual(main.lineage, cormoria.lineage)

    def test_shared_witnesses_reject_empty_party_and_changed_inventory(self) -> None:
        descriptor = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()
        save = read_flash(self.main)
        party = logical_field(save, descriptor, 0x0101)
        witness = {"field_id": 0x0101, "sha256": hashlib.sha256(party).hexdigest(),
                   "offset": 0, "size": 1, "min_nonzero_bytes": 1}
        with self.assertRaisesRegex(OracleFailure, "insufficiently populated"):
            check_shared_witnesses(save, descriptor, [witness])
        money = logical_field(save, descriptor, 0x0102)
        witness = dict(witness, field_id=0x0102, size=len(money), sha256=hashlib.sha256(money).hexdigest())
        receipt = check_shared_witnesses(save, descriptor, [witness])
        self.assertEqual(receipt[0]["sha256"], witness["sha256"])
        with self.assertRaisesRegex(OracleFailure, "changed"):
            check_shared_witnesses(save, descriptor, [dict(witness, sha256="0" * 64)])
        with self.assertRaisesRegex(OracleFailure, "duplicated"):
            check_shared_witnesses(save, descriptor, [witness, witness])
        with self.assertRaisesRegex(OracleFailure, "player data"):
            check_shared_witnesses(save, descriptor, [dict(witness, field_id=0x020B)])
        for invalid in (True, 0, len(money) + 1):
            with self.assertRaisesRegex(OracleFailure, "threshold"):
                check_shared_witnesses(save, descriptor, [dict(witness, min_nonzero_bytes=invalid)])
        for offset, size in ((True, 1), (-1, 1), (0, False), (0, 0), (len(money), 1)):
            with self.assertRaisesRegex(OracleFailure, "span"):
                check_shared_witnesses(save, descriptor, [dict(witness, offset=offset, size=size)])

    def test_populated_baseline_cannot_certify_an_empty_ferry_source(self) -> None:
        descriptor = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()
        save = read_flash(self.main)
        baseline = bytearray(logical_field(save, descriptor, 0x0101))
        baseline[0] = 1  # Unit-only populated byte fixture, never a ROM save.
        witness = {"field_id": 0x0101, "sha256": hashlib.sha256(baseline).hexdigest(),
                   "offset": 0, "size": 1, "min_nonzero_bytes": 1}
        with mock.patch("live_harness_oracles.logical_field", return_value=bytes(baseline)):
            self.assertEqual(len(check_shared_witnesses(save, descriptor, [witness])), 1)
        with self.assertRaisesRegex(OracleFailure, "insufficiently populated"):
            check_travel_witnesses(save, save, descriptor, [witness], exact_source=True)
        with mock.patch("live_harness_oracles.logical_field", side_effect=[bytes(baseline), bytes(len(baseline))]):
            with self.assertRaisesRegex(OracleFailure, "insufficiently populated"):
                check_travel_witnesses(save, save, descriptor, [witness], exact_source=True)
        with self.assertRaisesRegex(OracleFailure, "exact ferry source"):
            check_travel_witnesses(save, save, descriptor, [witness], exact_source=False)

    def test_failed_witness_prevents_client_launch_and_seed_requests(self) -> None:
        import live_seed_players
        descriptor = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()
        save = read_flash(self.main)
        witness = {"field_id": 0x0101, "sha256": "0" * 64,
                   "offset": 0, "size": 1, "min_nonzero_bytes": 1}
        def reject(*args, **kwargs):
            check_shared_witnesses(save, descriptor, [witness])
        with mock.patch.object(journey, "preflight", side_effect=reject), \
             mock.patch.object(journey, "launch_signed_desktop") as launch:
            with self.assertRaisesRegex(OracleFailure, "insufficiently populated"):
                journey.launch({})
            launch.assert_not_called()
        with mock.patch.object(live_seed_players, "validate_lineages", side_effect=reject), \
             mock.patch.object(live_seed_players, "check_c_space"), \
             mock.patch.object(live_seed_players, "_call") as request:
            with self.assertRaisesRegex(OracleFailure, "insufficiently populated"):
                live_seed_players.seed({})
            request.assert_not_called()

    def test_rejects_corrupt_selected_sector(self) -> None:
        data = bytearray(self.main.read_bytes())
        data[0] ^= 1
        data[15 * 4096] ^= 1
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "damaged.sav"
            path.write_bytes(data)
            with self.assertRaisesRegex(OracleFailure, "neither Flash1M slot"):
                read_flash(path)

    def test_drive_does_not_accept_an_old_travel_journal(self) -> None:
        plan = {
            "players": [
                {"name": "a", "profile_localappdata": "a", "character_id": "a-id"},
                {"name": "b", "profile_localappdata": "b", "character_id": "b-id"},
            ],
            "legs": [{"name": "outbound", "source_world_id": 1,
                      "destination_world_id": 2, "portal_id": "to_region",
                      "inputs": [{"player": "a", "key": "gba_a",
                                  "expect_travel": ["a"], "timeout_ms": 1000}]}],
        }
        old = [(4, Path("old.json"), {"phase": "adopted"})]
        with tempfile.TemporaryDirectory() as folder, \
             mock.patch.object(journey, "preflight"), \
             mock.patch.object(journey, "_paths", return_value=(Path(folder), Path(folder))), \
             mock.patch.object(journey, "Win32Adapter", return_value=object()), \
             mock.patch.object(journey, "tap"), \
             mock.patch.object(journey, "_journal_candidates", return_value=old), \
             mock.patch.object(journey, "capture_game"), \
             mock.patch.object(journey.psutil, "pid_exists", return_value=True), \
             mock.patch.object(journey.time, "monotonic", side_effect=[0, 2]):
            with self.assertRaisesRegex(journey.HarnessFailure, "no fresh signed-client travel journal"):
                journey.drive(plan, "outbound", {"a": 123})

    def test_rekey_decodes_money_coins_bag_and_stats(self) -> None:
        key = 0x11223344
        self.assertEqual(_logical_value(0x0102, (500 ^ key).to_bytes(4, "little"), key),
                         (500).to_bytes(4, "little"))
        self.assertEqual(_logical_value(0x0103, (99 ^ (key & 0xFFFF)).to_bytes(2, "little"), key),
                         (99).to_bytes(2, "little"))
        bag = b"\x01\x00" + (4 ^ (key & 0xFFFF)).to_bytes(2, "little")
        self.assertEqual(_logical_value(0x0106, bag, key), b"\x01\x00\x04\x00")
        self.assertEqual(_logical_value(0x0113, (21 ^ key).to_bytes(4, "little"), key),
                         (21).to_bytes(4, "little"))

    def test_fresh_receipt_without_exact_source_capture_still_fails(self) -> None:
        plan = {"players": [{"name": "a", "profile_localappdata": "a", "character_id": "a"},
                            {"name": "b", "profile_localappdata": "b", "character_id": "b"}],
                "legs": [{"name": "outbound", "source_world_id": 1, "destination_world_id": 2,
                          "portal_id": "to_region", "inputs": [{"player": "a", "key": "gba_a",
                                                                  "expect_travel": ["a"], "timeout_ms": 1000}]}]}
        fresh = [(5, Path("fresh.json"), {"intent": {"source_save_sha256": "0" * 64},
                                         "stage": {"destination_save_sha256": "1" * 64}})]
        with tempfile.TemporaryDirectory() as folder, \
             mock.patch.object(journey, "preflight"), \
             mock.patch.object(journey, "check_c_space"), \
             mock.patch.object(journey, "_paths", return_value=(Path(folder), Path(folder))), \
             mock.patch.object(journey, "Win32Adapter", return_value=object()), \
             mock.patch.object(journey, "tap"), \
             mock.patch.object(journey, "_journal_candidates", side_effect=[[], [], fresh]), \
             mock.patch.object(journey, "capture_game"), \
             mock.patch.object(journey.psutil, "pid_exists", return_value=True), \
             mock.patch.object(journey.time, "monotonic", side_effect=[0, 2]):
            with self.assertRaisesRegex(journey.HarnessFailure, "missing exact captures.*source"):
                journey.drive(plan, "outbound", {"a": 123})

    def test_descriptor_covers_shared_pc_and_bag(self) -> None:
        payload = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()
        fields = {f["id"]: f for f in parse_schema_payload(payload)["fields"]}
        for field_id in (0x0101, 0x0105, 0x0106, 0x0301, 0x0302, 0x0303, 0x0304, 0x0403):
            self.assertEqual(fields[field_id]["ownership"], 1)

    def test_journal_discovery_selects_latest_matching_committed_leg(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            profile = Path(folder)
            journal_dir = profile / "Hoenn Sessions" / "paired-travel"
            journal_dir.mkdir(parents=True)
            base = {
                "character_id": "character-a", "phase": "committed",
                "intent": {"source_world_id": 1,
                           "request": {"portal_id": "to_cormoria"}},
                "stage": {"destination_world_id": 2,
                          "destination_save_sha256": "a" * 64},
            }
            older = dict(base, sequence=1)
            newer = dict(base, sequence=3)
            (journal_dir / "0001.json").write_text(json.dumps(older), encoding="utf-8")
            (journal_dir / "0003.json").write_text(json.dumps(newer), encoding="utf-8")
            found = journey._journal_candidates(
                profile,
                {"source_world_id": 1, "destination_world_id": 2,
                 "portal_id": "to_cormoria"},
                "character-a",
            )
            self.assertEqual(found[0][0], 3)
            self.assertEqual(found[0][1].name, "0003.json")

    def test_server_url_rejects_credentials_and_remote_by_default(self) -> None:
        with self.assertRaisesRegex(journey.HarnessFailure, "without credentials"):
            journey._health_url({"server_url": "http://user:secret@127.0.0.1:1"})
        with self.assertRaisesRegex(journey.HarnessFailure, "remote host"):
            journey._health_url({"server_url": "https://example.invalid"})

    @mock.patch.dict("os.environ", {"COOP_HARNESS_SERVER_URL": "http://127.0.0.1:18080"}, clear=False)
    def test_server_url_is_normalized_to_read_only_health_path(self) -> None:
        endpoint, parsed = journey._health_url({})
        self.assertEqual(endpoint, "http://127.0.0.1:18080/health/ready")
        self.assertEqual(parsed.hostname, "127.0.0.1")

    @mock.patch.object(journey, "checkpoint")
    @mock.patch.object(journey, "_paths", return_value=(Path("C:/release"), Path("D:/run")))
    @mock.patch.object(journey.urllib.request, "urlopen")
    def test_server_check_is_a_retryable_read_only_health_probe(self, urlopen, _paths, checkpoint):
        class Response:
            status = 200

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

        urlopen.side_effect = [journey.urllib.error.URLError("offline"), Response()]
        plan = {
            "server_url": "http://127.0.0.1:18080",
            "release_id": "fixture", "release_dir": "C:/release",
            "run_dir": "D:/run", "players": [{"name": "a"}, {"name": "b"}],
            "legs": [], "server_check_retries": 2,
        }
        with mock.patch.object(journey.time, "sleep"):
            result = journey.check_server(plan)
        self.assertTrue(result["ready"])
        self.assertEqual(result["attempts"], 2)
        self.assertEqual(urlopen.call_count, 2)
        checkpoint.assert_called_once()

    def test_group_evidence_is_redacted_to_oracle_fields(self) -> None:
        safe = journey._sanitize_group_evidence({
            "group_id": "group-a", "members": [{"character_id": "a", "token": "secret"}],
            "world_zone": {"region": "CORMORIA", "session_id": "secret"},
            "access_token": "secret",
        })
        self.assertEqual(safe, {
            "group_id": "group-a", "members": [{"character_id": "a"}],
            "world_zone": {"region": "CORMORIA"},
        })


if __name__ == "__main__":
    unittest.main()
