import json
import copy
import hashlib
import importlib.util
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("scenery", ROOT / "tools/johto/import_scenery.py")
scenery = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scenery)

class JohtoSceneryTest(unittest.TestCase):
    def test_registration_counts_and_namespace(self):
        d = json.loads((ROOT / "data/johto/scenery_registration.json").read_text())
        self.assertEqual(len(d["layouts"]), 239)
        self.assertEqual(len(d["tilesets"]), 66)
        self.assertEqual(d["provenance"]["selection"]["asset_count"], 1534)
        self.assertEqual(d["layouts"][0]["target_layout_id"], "LAYOUT_NEW_BARK_TOWN")
        self.assertEqual(len({x["target_layout_id"] for x in d["layouts"]}), 239)
        self.assertEqual(len({x["target_name"] for x in d["layouts"]}), 239)

    def test_generated_layout_ordinals_and_geometry(self):
        d = json.loads((ROOT / "data/johto/scenery_registration.json").read_text())
        self.assertEqual([x["ordinal"] for x in d["layouts"] if x["ordinal"] >= 785], list(range(785, 1024)))
        self.assertTrue(all(x["width"] > 0 and x["height"] > 0 for x in d["layouts"]))
        self.assertTrue(all(x["border_width"] == 2 and x["border_height"] == 2 for x in d["layouts"]))

    def test_all_selected_assets_have_pinned_bytes(self):
        d = json.loads((ROOT / "data/johto/asset_manifest.json").read_text())
        assets = [a for group in ("layouts", "tilesets") for e in d[group] for a in e["assets"]]
        self.assertEqual(len(assets), 1534)
        for asset in assets:
            raw = (ROOT / "data/johto/scenery" / asset["path"]).read_bytes()
            self.assertEqual(hashlib.sha256(raw).hexdigest(), asset["output_sha256"], asset["path"])
            self.assertEqual(len(raw), asset["output_size"])

    def test_host_prefix_and_imported_tail_reject_mutations(self):
        assets = json.loads((ROOT / "data/johto/asset_manifest.json").read_text())
        original = json.loads((ROOT / "data/layouts/layouts.json").read_text())
        mutations = []
        changed = copy.deepcopy(original)
        changed["layouts"][0], changed["layouts"][1] = changed["layouts"][1], changed["layouts"][0]
        mutations.append(changed)
        changed = copy.deepcopy(original); changed["layouts"][10]["width"] += 1; mutations.append(changed)
        changed = copy.deepcopy(original); changed["layouts"][786], changed["layouts"][787] = changed["layouts"][787], changed["layouts"][786]; mutations.append(changed)
        changed = copy.deepcopy(original); changed["layouts"].append(changed["layouts"][-1]); mutations.append(changed)
        changed = copy.deepcopy(original); changed["layouts"][786]["id"] = "LAYOUT_JOHTO_UNKNOWN"; mutations.append(changed)
        changed = copy.deepcopy(original); changed["unexpected"] = True; mutations.append(changed)
        for changed in mutations:
            with self.subTest(changed=changed.get("unexpected", False)):
                with self.assertRaises(scenery.ImportError):
                    scenery._layout_records(assets, changed)
        result, _, _ = scenery._layout_records(assets, original)
        self.assertEqual(result, original)

    def test_check_rejects_missing_and_stale_outputs_without_writes(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "asset.bin"
            with patch.object(scenery, "_plan", return_value=({output: b"expected"}, {}, {})):
                self.assertEqual(scenery.main(["--donor-root", folder, "--check"]), 1)
                self.assertFalse(output.exists())
                output.write_bytes(b"drift")
                self.assertEqual(scenery.main(["--donor-root", folder, "--check"]), 1)
                self.assertEqual(scenery.main(["--donor-root", folder, "--write"]), 1)
                self.assertEqual(output.read_bytes(), b"drift")
                output.write_bytes(b"expected")
                self.assertEqual(scenery.main(["--donor-root", folder, "--check"]), 0)

    def test_metadata_rejects_unknown_fields_and_unmapped_callbacks(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "src/data/tilesets/headers.h"
            path.parent.mkdir(parents=True)
            path.write_text("const struct Tileset gTileset_Test = { .unknown = 1, };")
            with self.assertRaises(scenery.ImportError):
                scenery._tileset_metadata(Path(folder), "gTileset_Test", {"kind": "primary"}, {})
            path.write_text("const struct Tileset gTileset_Test = { .swapPalettes = SWAP_PAL(8), };")
            with self.assertRaises(scenery.ImportError):
                scenery._tileset_metadata(Path(folder), "gTileset_Test", {"kind": "primary", "callback": "InitTilesetAnim_Unknown"}, {})
            callback, metadata = scenery._tileset_metadata(Path(folder), "gTileset_Test", {"kind": "primary", "callback": None}, {})
            self.assertIsNone(callback)
            self.assertEqual(metadata["swapPalettes"], 2)

    def test_source_drift_rejects_before_any_output_write(self):
        with tempfile.TemporaryDirectory() as folder:
            donor = Path(folder)
            (donor / "source.bin").write_bytes(b"drift")
            asset = {"path": "source.bin", "conversion": "identity", "source_sha256": hashlib.sha256(b"original").hexdigest(), "output_sha256": hashlib.sha256(b"original").hexdigest()}
            class AssetModule:
                @staticmethod
                def _behavior_map(*args):
                    return {}, {}
            with self.assertRaisesRegex(scenery.ImportError, "pinned asset hash mismatch"):
                scenery._source_assets({"layouts": [{"assets": [asset]}], "tilesets": []}, donor, AssetModule)
            self.assertEqual(list(donor.iterdir()), [donor / "source.bin"])

    def test_callbacks_are_closed_or_null(self):
        d = json.loads((ROOT / "data/johto/scenery_registration.json").read_text())
        allowed = {None, "InitTilesetAnim_JohtoGeneral", "InitTilesetAnim_JohtoNationalPark", "InitTilesetAnim_JohtoEcruteakTheater", "InitTilesetAnim_JohtoAzaleaGym", "InitTilesetAnim_JohtoBlackthornGym"}
        self.assertTrue(all(x["callback"] in allowed for x in d["tilesets"]))
        by_source = {x["source_symbol"]: x["callback"] for x in d["tilesets"]}
        self.assertIsNone(by_source["gTileset_SootopolisGym"])
        self.assertIsNone(by_source["gTileset_BikeShop"])

    def test_u32_attribute_conversion_is_recorded(self):
        d = json.loads((ROOT / "data/johto/asset_manifest.json").read_text())
        attrs = [a for t in d["tilesets"] for a in t["assets"] if a["path"].endswith("metatile_attributes.bin")]
        self.assertEqual(len(attrs), 66)
        self.assertTrue(all(a["conversion"] == "u16-attribute-to-u32" and a["output_size"] == 2 * a["source_size"] for a in attrs))

if __name__ == "__main__":
    unittest.main()
