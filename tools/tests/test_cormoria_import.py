"""The Cormoria stage is authenticated, complete and isolated from live maps."""
from __future__ import annotations

import copy
import hashlib
import json
import os
import tempfile
import unittest
from pathlib import Path

from tools.cormoria import import_world

DONOR = Path(os.environ.get(
    "CORMORIA_DONOR",
    "C:/Users/Mayor/AppData/Local/Temp/dreamstone-assessment-bfce03f1/dreamstone-mysteries",
))


class ContractTests(unittest.TestCase):
    def test_path_escape_and_source_hashes_fail_closed(self):
        for path in ("../secret", "/absolute", "C:/drive", "a\\b"):
            with self.subTest(path=path), self.assertRaises(import_world.ImportError):
                import_world.safe_relative(path)
        with tempfile.TemporaryDirectory() as directory:
            donor = Path(directory)
            source = donor / "map.json"
            source.write_bytes(b"pinned")
            record = {"map.json": {"bytes": 6, "sha256": hashlib.sha256(b"pinned").hexdigest()}}
            self.assertEqual(import_world.source_bytes(donor, "map.json", record), b"pinned")
            source.write_bytes(b"altered")
            with self.assertRaisesRegex(import_world.ImportError, "hash mismatch"):
                import_world.source_bytes(donor, "map.json", record)
            source.unlink()
            with self.assertRaisesRegex(import_world.ImportError, "missing"):
                import_world.source_bytes(donor, "map.json", record)
            with self.assertRaisesRegex(import_world.ImportError, "omitted"):
                import_world.source_bytes(donor, "map.json", {})

    def test_identity_collision_and_manifest_pins(self):
        region, symbols, sources = import_world.load_manifests()
        self.assertEqual([len(region[key]) for key in ("maps", "layouts")], [165, 165])
        self.assertEqual(len(sources["assets"]), 2371)
        changed = copy.deepcopy(region)
        changed["maps"][1]["target_id"] = changed["maps"][0]["target_id"]
        with self.assertRaisesRegex(import_world.ImportError, "collision"):
            import_world.identities(changed, symbols)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / import_world.MANIFESTS
            destination.mkdir(parents=True)
            for name in import_world.PINNED_SHA256:
                canonical = import_world.manifest_bytes(import_world.ROOT, name)
                (destination / name).write_bytes(canonical.replace(b"\n", b"\r\n"))
                self.assertEqual(import_world.manifest_bytes(root, name), canonical)
            import_world.load_manifests(root)
            (destination / "region_manifest.json").write_bytes(b"{}")
            with self.assertRaisesRegex(import_world.ImportError, "pinned manifest changed"):
                import_world.load_manifests(root)

    def test_script_rewrite_preserves_dialogue(self):
        text = 'Entry::\n\tmsgbox Text\nText::\n\t.string "Entry and MAP_ROUTE1$"\n'
        changed = import_world.rewrite_script(text, {"Entry": "Cormoria_Entry", "MAP_ROUTE1": "MAP_CORMORIA_ROUTE1"})
        self.assertIn("Cormoria_Entry::", changed)
        self.assertIn('"Entry and MAP_ROUTE1$"', changed)


@unittest.skipUnless(DONOR.is_dir(), "pinned Dreamstone donor checkout unavailable")
class PinnedCampaignTests(unittest.TestCase):
    def test_stage_complete_deterministic_and_does_not_register_live_content(self):
        root = import_world.ROOT
        protected = [root / "data/maps/map_groups.json", root / "data/layouts/layouts.json",
                     root / "data/event_scripts.s"]
        before = {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in protected if path.is_file()}
        with tempfile.TemporaryDirectory() as directory:
            one, two = (Path(directory) / name for name in ("one", "two"))
            first = import_world.build_stage(DONOR, one)
            second = import_world.build_stage(DONOR, two)
            self.assertEqual(first, second)
            self.assertEqual((one / "staging_manifest.json").read_bytes(),
                             (two / "staging_manifest.json").read_bytes())
            self.assertEqual((first["map_count"], first["layout_count"],
                              first["section_count"], first["tileset_count"]),
                             (165, 165, 51, 59))
            self.assertEqual(first["asset_recipe_count"], 2371)
            self.assertEqual(first["namespaced_script_source_count"], 188)
            self.assertEqual(len(list((one / "provenance").glob("*.json"))), 3)
            self.assertFalse(first["runtime_ready"])
            self.assertEqual(len(list((one / "maps").glob("*/map.json"))), 165)
            self.assertEqual(len(list((one / "layouts").glob("*/layout.json"))), 165)
            region, _, _ = import_world.load_manifests()
            self.assertTrue(all((one / "source/data/maps" / item["source_name"] / "scripts.inc").is_file()
                                for item in region["maps"]))
            script = (one / "namespaced_scripts/data/maps/CarabrueTown/scripts.inc").read_text()
            self.assertIn("Cormoria_CarabrueTown_EventScript_PoliceRoadBlock", script)
            map_data = json.loads((one / "maps/Cormoria_CarabrueTown/map.json").read_bytes())
            self.assertEqual(map_data["id"], "MAP_CORMORIA_CARABRUE_TOWN")
            self.assertEqual(map_data["rom_world"], "cormoria")
            self.assertEqual(map_data["connections"][0]["map"], "MAP_CORMORIA_ROUTE1")
            self.assertTrue(all(path["path"].startswith("source/src/data/")
                                for path in first["files"] if path["path"].startswith("source/src/")))
            self.assertFalse(any(path["path"].startswith(("source/src/data/pokemon/",
                                                             "source/src/data/union_room",
                                                             "source/src/data/party_menu",
                                                             "source/src/data/battle_frontier/"))
                                 for path in first["files"]))
            self.assertTrue((one / "adapter_obligations.json").is_file())
            self.assertEqual({str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                              for path in protected if path.is_file()}, before)
            with self.assertRaisesRegex(import_world.ImportError, "overwrite"):
                import_world.build_stage(DONOR, one)

    def test_output_inside_live_checkout_is_rejected(self):
        with self.assertRaisesRegex(import_world.ImportError, "outside"):
            import_world.build_stage(DONOR, import_world.ROOT / "data/cormoria/stage")


if __name__ == "__main__":
    unittest.main()
