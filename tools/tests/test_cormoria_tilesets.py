"""Checks for the authenticated, isolated Cormoria tileset preview."""

from __future__ import annotations

import copy
import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.cormoria import import_world, register_tilesets


class CormoriaTilesetPreviewTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.stage = Path(os.environ.get(
            "CORMORIA_STAGE", Path.home() / ".codex/cormoria-swarm-artifacts/content-stage-20260923-v5"))
        cls.foundation = Path(os.environ.get(
            "CORMORIA_GENERAL_FOUNDATION", Path.home() / ".codex/cormoria-swarm-artifacts/general-foundation-20260923-v1"))
        cls.rules = Path(os.environ.get(
            "CORMORIA_DONOR_RULES", Path.home() / ".codex/cormoria-swarm-artifacts/graphics_file_rules-donor-f7997186.mk"))
        cls.anims = Path(os.environ.get(
            "CORMORIA_DONOR_ANIMS", Path.home() / ".codex/cormoria-swarm-artifacts/tileset_anims-donor-f7997186.c"))
        if not cls.stage.is_dir() or not cls.foundation.is_dir() or not cls.rules.is_file() or not cls.anims.is_file():
            raise unittest.SkipTest("authenticated Cormoria tileset inputs are unavailable")

    def test_exact_manifest_binding_and_general_graphics(self) -> None:
        rendered = register_tilesets.render(self.stage, self.foundation, self.rules, self.anims)
        code = rendered["tilesets.c"].decode()
        plan = json.loads(rendered["asset_plan.json"])
        self.assertIn("const struct Tileset Cormoria_gTileset_AncientMirroh", code)
        self.assertIn("const u16 Cormoria_gMetatiles_General[]", code)
        self.assertIn("const struct Tileset Cormoria_gTileset_General", code)
        self.assertIn("const u32 Cormoria_gTilesetTiles_General", code)
        self.assertEqual(len(plan["registered_tilesets"]), 59)
        self.assertEqual(len(set(plan["registered_tilesets"])), 59)
        self.assertTrue(all(f"const struct Tileset {name} " in code
                            for name in plan["registered_tilesets"]))
        self.assertEqual({row["callback"] for row in plan["unresolved"]},
                         set())
        self.assertEqual(plan["callback_rebindings"][0]["donor_callback"], "InitTilesetAnim_Snow")
        self.assertIn(".callback = InitTilesetAnim_General,", code)
        self.assertNotIn(".callback = InitTilesetAnim_Snow,", code)
        self.assertEqual(len(plan["recipes"]), 1058)
        shop = next(row for row in plan["recipes"] if row["requested"] ==
                    "data/tilesets/secondary/shop/tiles.4bpp.lz")
        self.assertEqual(shop["num_tiles"], 502)
        self.assertEqual(sum(row["origin"] == "general-foundation" for row in plan["recipes"]), 17)
        self.assertNotIn("extern void InitTilesetAnim_Snow(void);", code)
        self.assertNotIn("void InitTilesetAnim_Snow(void)\n{", code)

    def test_manifest_identity_drift_rejected(self) -> None:
        original = import_world.load_manifests
        def changed(root: Path):
            region, symbols, sources = original(root)
            region = copy.deepcopy(region)
            row = next(row for row in region["tilesets"] if row["source_symbol"] == "gTileset_AncientMirroh")
            row["target_symbol"] = "gTileset_AncientMirroh"
            return region, symbols, sources
        with mock.patch.object(register_tilesets.import_world, "load_manifests", side_effect=changed):
            with self.assertRaisesRegex(register_tilesets.TilesetRegistrationError, "binding drifted"):
                register_tilesets.render(self.stage, self.foundation, self.rules, self.anims)

    def test_host_path_collision_isolated(self) -> None:
        rendered = register_tilesets.render(self.stage, self.foundation, self.rules, self.anims)
        plan = json.loads(rendered["asset_plan.json"])
        general = next(row for row in plan["recipes"]
                       if row["requested"] == "data/tilesets/primary/general/metatiles.bin")
        self.assertTrue((register_tilesets.ROOT / general["requested"]).exists())
        self.assertEqual(general["target"], "data/tilesets/cormoria/primary/general/metatiles.bin")
        self.assertNotEqual(general["requested"], general["target"])
        self.assertTrue(all(row["requested"] != row["target"] and
                            row["target"].startswith("data/tilesets/cormoria/")
                            for row in plan["recipes"]))
        self.assertNotIn('INCBIN_U16("data/tilesets/primary/general/metatiles.bin")',
                         rendered["tilesets.c"].decode())

    def test_staged_asset_tamper_rejected_even_with_stage_hash_record(self) -> None:
        original = register_tilesets._source
        def changed(stage: Path, relative: str, indexed: dict):
            data = original(stage, relative, indexed)
            if relative == "data/tilesets/primary/general/metatiles.bin":
                return data[:-1] + bytes([data[-1] ^ 1])
            return data
        with mock.patch.object(register_tilesets, "_source", side_effect=changed):
            with self.assertRaisesRegex(register_tilesets.TilesetRegistrationError, "donor asset hash mismatch"):
                register_tilesets.render(self.stage, self.foundation, self.rules, self.anims)

    def test_general_source_tamper_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            supplement = Path(directory) / "foundation"
            shutil.copytree(self.foundation, supplement)
            palette = supplement / "source/data/tilesets/primary/general/palettes/00.pal"
            palette.write_bytes(palette.read_bytes() + b"tamper")
            with self.assertRaisesRegex(register_tilesets.TilesetRegistrationError,
                                        "General source hash mismatch"):
                register_tilesets.render(self.stage, supplement, self.rules, self.anims)

    def test_donor_rules_tamper_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            altered = Path(directory) / "graphics_file_rules.mk"
            altered.write_bytes(self.rules.read_bytes() + b"tamper")
            with self.assertRaisesRegex(register_tilesets.TilesetRegistrationError,
                                        "donor graphics rules hash mismatch"):
                register_tilesets.render(self.stage, self.foundation, altered, self.anims)

    def test_snow_rebinding_requires_authenticated_equivalent_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            altered = Path(directory) / "tileset_anims.c"
            altered.write_bytes(self.anims.read_bytes() + b"tamper")
            with self.assertRaisesRegex(register_tilesets.TilesetRegistrationError,
                                        "donor animation source hash mismatch"):
                register_tilesets.render(self.stage, self.foundation, self.rules, altered)

    def test_snow_rebinding_requires_matching_animation_frame(self) -> None:
        original = register_tilesets._source
        def changed(stage: Path, relative: str, indexed: dict):
            data = original(stage, relative, indexed)
            if relative == "data/tilesets/primary/general/anim/flower/0.png":
                return data + b"tamper"
            return data
        with mock.patch.object(register_tilesets, "_source", side_effect=changed):
            with self.assertRaisesRegex(register_tilesets.TilesetRegistrationError,
                                        "General animation frame differs"):
                register_tilesets.render(self.stage, self.foundation, self.rules, self.anims)

    def test_deterministic_output_and_external_fresh_path(self) -> None:
        self.assertEqual(register_tilesets.render(self.stage, self.foundation, self.rules, self.anims),
                         register_tilesets.render(self.stage, self.foundation, self.rules, self.anims))
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "new-preview"
            command = ["--stage", str(self.stage), "--general-foundation", str(self.foundation),
                       "--donor-rules", str(self.rules), "--donor-anims", str(self.anims), "--output", str(output)]
            self.assertEqual(register_tilesets.main(command), 0)
            self.assertEqual(register_tilesets.main(command), 1)
            self.assertEqual((output / "tilesets.c").read_bytes(),
                             register_tilesets.render(self.stage, self.foundation, self.rules, self.anims)["tilesets.c"])
        self.assertEqual(register_tilesets.main(["--stage", str(self.stage), "--general-foundation", str(self.foundation),
                                                "--donor-rules", str(self.rules),
                                                "--donor-anims", str(self.anims),
                                                "--output", str(register_tilesets.ROOT / "inside")]), 1)
        self.assertEqual(register_tilesets.main(["--stage", str(self.stage), "--general-foundation", str(self.foundation),
                                                "--donor-rules", str(self.rules),
                                                "--donor-anims", str(self.anims),
                                                "--output", str(self.stage / "inside")]), 1)


if __name__ == "__main__":
    unittest.main()
