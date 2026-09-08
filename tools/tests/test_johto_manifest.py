"""Focused contract tests for the Johto manifest tooling."""

from __future__ import annotations

import json
import copy
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.johto import region_manifest as manifest


DONOR = Path(os.environ["JOHTO_DONOR"]) if os.environ.get("JOHTO_DONOR") else None


class ManifestContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if DONOR is None:
            raise unittest.SkipTest("set JOHTO_DONOR to run pinned corpus checks")
        cls.ledger = manifest.build_manifest(DONOR)

    def test_deterministic_output_and_counts(self):
        self.assertEqual(self.ledger, manifest.build_manifest(DONOR))
        self.assertEqual(self.ledger["selection"]["selected_count"], 239)
        self.assertEqual(self.ledger["selection"]["classified_count"], 954)
        self.assertEqual(self.ledger["selection"]["excluded_count"], 715)
        self.assertEqual(len(self.ledger["maps"]), 239)
        self.assertEqual(len(self.ledger["registrations"]), 954)

    def test_new_bark_and_signed_boundary(self):
        maps = self.ledger["maps"]
        self.assertEqual(maps[0]["source_map"], "MAP_NEW_BARK_TOWN")
        self.assertEqual(maps[0]["proposed_host"], {"group": 75, "index": 0})
        self.assertEqual(maps[127]["proposed_host"], {"group": 75, "index": 127})
        self.assertEqual(maps[128]["proposed_host"], {"group": 76, "index": 0})
        self.assertEqual(maps[-1]["proposed_host"], {"group": 76, "index": 110})
        self.assertTrue(all(m["proposed_host"]["group"] <= 127 and
                            m["proposed_host"]["index"] <= 127 for m in maps))

    def test_sections_aliases_and_reservations(self):
        sections = self.ledger["sections"]
        self.assertEqual(sections["source_count"], 57)
        self.assertEqual(sections["johto_count"], 41)
        self.assertEqual(sections["kanto_count"], 1)
        self.assertEqual(len(sections["entries"]), 57)
        self.assertEqual(len(self.ledger["section_aliases"]), 15)
        self.assertEqual(self.ledger["host_identity"]["reserved_section_ids"], [250, 251, 252, 253, 254, 255])
        ids = {entry["target_id"] for entry in sections["entries"]}
        self.assertNotIn(250, ids)
        self.assertTrue(all(210 <= value <= 249 for value in ids if value not in (132, 209)))
        reception = next(e for e in sections["entries"] if e["source_symbol"] == "MAPSEC_VICTORY_ROAD")
        self.assertEqual((reception["target_symbol"], reception["target_id"]),
                         ("MAPSEC_KANTO_VICTORY_ROAD", 132))
        reception_map = next(m for m in self.ledger["maps"]
                             if m["source_map"] == "MAP_RECEPTION_GATE")
        self.assertEqual(reception_map["region"], "REGION_KANTO")
        self.assertEqual(reception_map["resolved_section"]["alias_target"], "RECEPTION_GATE")

    def test_edges_are_retained_and_classified(self):
        edges = self.ledger["external_edges"]
        self.assertGreater(len(edges), 0)
        self.assertTrue(all(edge["classification"] in
                            {"required_host_adapter", "excluded_debug_edge"} for edge in edges))
        self.assertTrue(any(edge["classification"] == "required_host_adapter" for edge in edges))
        self.assertTrue(any(edge["classification"] == "excluded_debug_edge" for edge in edges))

    def test_donor_pin_rejection(self):
        with mock.patch.object(manifest, "_git_revision", return_value=("wrong", "tree")):
            with self.assertRaisesRegex(manifest.ManifestError, "revision mismatch"):
                manifest.build_manifest(DONOR)

    def test_missing_and_duplicate_references_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data/maps").mkdir(parents=True)
            (root / "data/maps/map_groups.json").write_text(
                json.dumps({"group_order": ["group"], "group": ["same", "same"]}),
                encoding="utf-8")
            with self.assertRaisesRegex(manifest.ManifestError, "duplicate map name"):
                manifest._map_groups(root)
            (root / "data/maps/map_groups.json").write_text(
                json.dumps({"group_order": ["group"], "group": ["missing"]}),
                encoding="utf-8")
            (root / "data/layouts").mkdir(parents=True)
            (root / "data/layouts/layouts.json").write_text('{"layouts": []}', encoding="utf-8")
            with mock.patch.object(manifest, "_git_revision",
                                   return_value=(manifest.DONOR_REVISION, "tree")):
                with self.assertRaisesRegex(manifest.ManifestError, "missing map registration source"):
                    manifest.build_manifest(root)

    def test_check_detects_stale_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            stale = Path(directory) / "region_manifest.json"
            stale.write_text("{}\n", encoding="utf-8")
            with mock.patch.object(manifest, "OUTPUT", stale):
                self.assertEqual(manifest.main(["--donor", str(DONOR), "--check"]), 2)

    def test_canonical_output_has_no_machine_donor_path(self):
        self.assertNotIn("donor_path", self.ledger["provenance"])

    def test_equivalent_donor_paths_have_equal_canonical_content(self):
        self.assertEqual(self.ledger, manifest.build_manifest(DONOR / "."))

    def test_rocket_hideout_symbols_are_qualified_without_changing_sources(self):
        expected = {
            "MAP_ROCKET_HIDEOUT_B1F": "MAP_JOHTO_ROCKET_HIDEOUT_B1F",
            "MAP_ROCKET_HIDEOUT_B2F": "MAP_JOHTO_ROCKET_HIDEOUT_B2F",
            "MAP_ROCKET_HIDEOUT_B3F": "MAP_JOHTO_ROCKET_HIDEOUT_B3F",
        }
        for source_id, proposed_id in expected.items():
            entry = next(item for item in self.ledger["maps"]
                         if item["source_map"] == source_id)
            self.assertEqual(entry["proposed_map"]["map_id"], proposed_id)


class ManifestSyntheticContractTests(unittest.TestCase):
    def test_explicit_edge_target_sets_and_missing_edges(self):
        self.assertEqual(
            manifest._target_classification("MAP_DYNAMIC", set(), set(), set(),
                                            "MAP_TEST", 3),
            "required_host_adapter",
        )
        with self.assertRaisesRegex(manifest.ManifestError, "unknown edge target: MAP_TEST edge 4"):
            manifest._target_classification("MAP_UNKNOWN", set(), set(), set(),
                                            "MAP_TEST", 4)
        with self.assertRaisesRegex(manifest.ManifestError, "missing edge target: MAP_TEST edge 5"):
            manifest._target_classification(None, set(), set(), set(), "MAP_TEST", 5)

    def test_nonselected_known_donor_target_is_not_debug(self):
        with self.assertRaisesRegex(manifest.ManifestError, "unknown edge target: MAP_TEST edge 6"):
            manifest._target_classification(
                "MAP_PALLET_TOWN", set(), {"MAP_PALLET_TOWN"}, set(), "MAP_TEST", 6
            )

    def test_required_adapter_retains_symbol_and_pending_resolution(self):
        edges = manifest._edge_records(
            {"warp_events": [{"dest_map": "MAP_DYNAMIC", "dest_warp_id": "WARP_ID_DYNAMIC"}]},
            set(), set(), set(), "MAP_TEST",
        )
        self.assertEqual(edges[0]["adapter"], {
            "donor_symbol": "MAP_DYNAMIC", "resolution": "pending_runtime_resolution",
        })

    def test_host_identity_baseline_rejects_group_and_within_group_reorder(self):
        baseline = manifest._host_identity(manifest.HOST_IDENTITY_BASELINE)
        changed = copy.deepcopy(baseline)
        changed["group_order"] = list(reversed(changed["group_order"]))
        with self.assertRaisesRegex(manifest.ManifestError, "group order"):
            manifest._assert_host_identity(changed)
        changed = copy.deepcopy(baseline)
        first_group = changed["group_order"][0]
        changed["groups"][first_group][0], changed["groups"][first_group][1] = (
            changed["groups"][first_group][1], changed["groups"][first_group][0]
        )
        with self.assertRaisesRegex(manifest.ManifestError, "identity/order"):
            manifest._assert_host_identity(changed)

    def test_dirty_donor_inputs_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            donor = Path(directory)
            (donor / ".git").mkdir()
            with mock.patch.object(manifest.subprocess, "check_output", return_value=" M data/maps/Foo/map.json\n"):
                with self.assertRaisesRegex(manifest.ManifestError, "dirty map/layout inputs"):
                    manifest._assert_donor_clean(donor)

    def test_proposed_section_collisions_are_rejected(self):
        with self.assertRaisesRegex(manifest.ManifestError, "section IDs collide"):
            manifest._assert_section_allocation({
                "MAPSEC_JOHTO_A": {"id": 211},
                "MAPSEC_JOHTO_B": {"id": 211},
            })

    def test_proposed_map_symbol_and_numeric_collisions_are_rejected(self):
        baseline = manifest._host_identity(manifest.HOST_IDENTITY_BASELINE)
        with self.assertRaisesRegex(manifest.ManifestError, "symbol collides with host baseline"):
            manifest._assert_proposed_map_identity(
                "MAP_ROCKET_HIDEOUT_B1F", 76, 86, "MAP_TEST", baseline, set(), set()
            )
        with self.assertRaisesRegex(manifest.ManifestError, "numeric identity collides with another proposal"):
            manifest._assert_proposed_map_identity(
                "MAP_SYNTHETIC_B", 75, 1, "MAP_SYNTHETIC_B", baseline,
                {"MAP_SYNTHETIC_A"}, {(75, 1)},
            )


if __name__ == "__main__":
    unittest.main()
