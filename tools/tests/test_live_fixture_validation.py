"""Admission tests use canonical synthetic unit bytes, never live save uploads."""
import base64
import copy
import json
import sys
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_fixture_validation as validation
import live_region_harness as journey
import live_seed_players as seeds
from live_harness_oracles import OracleFailure
from tools.tests import test_live_fixture_custody as unit_bytes


class FixtureAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.fixture = unit_bytes.CustodyTests()
        self.fixture.setUp()
        self.save = self.fixture.save
        self.descriptor = (ROOT / "coop/crates/coop-save/src/fixtures/player_transfer_v3.bin").read_bytes()
        self.fields = {0x0101: bytes(self.fixture.party), 0x010B: bytes(self.fixture.mail),
                       0x010D: bytes(self.fixture.daycare)}
        self.witnesses = self.fixture.verify()["shared_witnesses"]
        self.player = {"custody_recipe": self.fixture.recipe, "shared_witnesses": self.witnesses}

    def verify(self, player=None):
        field = lambda s, d, fid: self.fields[fid]
        with mock.patch("live_harness_oracles.logical_field", side_effect=field), \
             mock.patch("live_fixture_custody.logical_field", side_effect=field):
            return validation.validate_player_fixture(self.save, self.descriptor,
                                                      self.player if player is None else player)

    def test_valid_custody_and_population_only_compatibility(self):
        self.assertEqual(len(self.verify()), 3)
        self.assertEqual(self.verify({}), [])
        self.assertEqual(len(self.verify({"shared_witnesses": self.witnesses[:1]})), 1)

    def test_missing_stale_partial_duplicate_and_threshold_are_rejected(self):
        for change in ("missing", "stale", "partial", "duplicate", "threshold"):
            player = copy.deepcopy(self.player)
            if change == "missing":
                player["shared_witnesses"].pop(0)
            elif change == "duplicate":
                player["shared_witnesses"].append(copy.deepcopy(player["shared_witnesses"][0]))
            elif change == "stale":
                player["shared_witnesses"][0]["sha256"] = "0" * 64
            elif change == "partial":
                player["shared_witnesses"][0]["size"] = 1
            else:
                player["shared_witnesses"][0]["min_nonzero_bytes"] = 2
            with self.subTest(change=change), self.assertRaises(OracleFailure):
                self.verify(player)

    def test_changed_attachment_preserving_custody_fields_fails(self):
        for boundary in ("index", "item"):
            party = bytearray(self.fixture.party)
            if boundary == "index":
                party[89] = 255
            else:
                party[4:84] = self.fixture.box(17, 0)
            self.fields[0x0101] = bytes(party)
            with self.subTest(boundary=boundary), self.assertRaisesRegex(OracleFailure, "changed"):
                self.verify()

    def test_present_null_malformed_and_future_abi_fail_closed(self):
        for recipe in (None, [], {}, dict(self.fixture.recipe, abi="future-family")):
            with self.subTest(recipe=recipe), self.assertRaises(OracleFailure):
                self.verify(dict(self.player, custody_recipe=recipe))

    def test_seed_validates_final_source_before_first_api_mutation(self):
        self.save.generation = 1
        self.save.lineage = "unit"
        players = [dict(self.player, name=name, source_sha256=self.save.sha256,
                        seed_lineage=[{"path": "unit.sav", "sha256": self.save.sha256}]) for name in ("a", "b")]
        plan = {"players": players, "seed_world_id": 1, "server_catalog_sha256": "unit"}
        catalog = {"worlds": [{"world_id": 1}], "shared_player_descriptor_hex": self.descriptor.hex()}
        rejected_player = getattr(self, "rejected_player", 1)
        results = [[]] * (rejected_player - 1) + [OracleFailure("custody rejected")]
        with mock.patch.object(seeds, "check_c_space"), \
             mock.patch.object(seeds, "_paths", return_value=(Path("release"), Path("run"))), \
             mock.patch.object(seeds, "require_hash"), \
             mock.patch.object(seeds, "_read_json", return_value=catalog), \
             mock.patch.object(seeds, "read_flash", return_value=self.save), \
             mock.patch.object(seeds, "validate_player_fixture", side_effect=results) as gate, \
             mock.patch.object(seeds, "_call") as api:
            with self.assertRaisesRegex(OracleFailure, "custody rejected"):
                seeds.seed(plan, register=True)
            self.assertEqual(gate.call_args_list, [mock.call(self.save, self.descriptor, player)
                                                   for player in players[:rejected_player]])
            api.assert_not_called()

    def test_seed_second_player_rejection_prevents_every_api_call(self):
        self.rejected_player = 2
        self.test_seed_validates_final_source_before_first_api_mutation()

    def test_preflight_validates_pinned_source_before_launch(self):
        player = dict(self.player, name="a", character_id="a", source_save="unit.sav",
                      source_sha256=self.save.sha256)
        plan = {"players": [player, dict(player, name="b", character_id="b")],
                "release_id": "unit", "release_public_key_hex": "00" * 32,
                "envelope_sha256": "hash", "catalog_sha256": "hash", "server_catalog_sha256": "hash"}
        signed = {"release_id": "unit", "artifacts": []}
        envelope = {"payload": base64.b64encode(json.dumps(signed).encode()).decode(),
                    "signature": base64.b64encode(bytes(64)).decode()}
        def read_json(path, label):
            if label == "release envelope":
                return envelope
            raise AssertionError(label)
        original_loads = json.loads
        def loads(value, **kwargs):
            if value == "unit-catalog":
                return {"worlds": []}
            if value == "unit-server":
                return {"shared_player_descriptor_hex": self.descriptor.hex()}
            return original_loads(value, **kwargs)
        def text(path, **kwargs):
            return "unit-server" if path.name == "server-build-catalog.json" else "unit-catalog"
        rejected_player = getattr(self, "rejected_player", 1)
        results = [[]] * (rejected_player - 1) + [OracleFailure("custody rejected")]
        actual_preflight = journey.preflight
        with mock.patch.object(journey, "_validate_plan_shape"), \
             mock.patch.object(journey, "preflight", side_effect=lambda p: actual_preflight(p, profiles_ready=False)), \
             mock.patch.object(journey, "_paths", return_value=(Path("release"), Path("run"))), \
             mock.patch.object(journey, "check_c_space"), \
             mock.patch.object(journey, "_read_json", side_effect=read_json), \
             mock.patch.object(journey.Ed25519PublicKey, "from_public_bytes"), \
             mock.patch.object(journey, "digest", return_value="hash"), \
             mock.patch.object(journey, "require_hash"), \
             mock.patch.object(Path, "read_text", text), \
             mock.patch.object(journey.json, "loads", side_effect=loads), \
             mock.patch.object(journey, "read_flash", return_value=self.save), \
             mock.patch.object(journey, "validate_player_fixture", side_effect=results) as gate, \
             mock.patch.object(journey, "launch_signed_desktop") as launch:
            with self.assertRaisesRegex(OracleFailure, "custody rejected"):
                journey.launch(plan)
            self.assertEqual(gate.call_args_list, [mock.call(self.save, self.descriptor, item)
                                                   for item in plan["players"][:rejected_player]])
            launch.assert_not_called()

    def test_preflight_second_player_rejection_prevents_every_launch(self):
        self.rejected_player = 2
        self.test_preflight_validates_pinned_source_before_launch()


if __name__ == "__main__":
    unittest.main()
