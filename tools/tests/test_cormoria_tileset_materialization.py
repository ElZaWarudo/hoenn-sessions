"""The tileset preview must only build from authenticated, isolated sources."""

from __future__ import annotations

import os
import json
import tempfile
import unittest
from pathlib import Path

from tools.cormoria import materialize_tileset_preview, register_tilesets


class CormoriaTilesetMaterializationTests(unittest.TestCase):
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

    def test_materialized_assets_have_isolated_paths_and_original_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            preview = root / "preview"
            preview.mkdir()
            for name, content in register_tilesets.render(self.stage, self.foundation, self.rules, self.anims).items():
                (preview / name).write_bytes(content)
            output = root / "assets"
            self.assertEqual(materialize_tileset_preview.materialize(
                self.stage, self.foundation, self.rules, self.anims, preview, output, source_only=True), 1058)
            self.assertTrue((output / "data/tilesets/cormoria/secondary/ancient_mirroh/tiles.png").is_file())
            self.assertTrue((output / "data/tilesets/cormoria/primary/general/metatiles.bin").is_file())
            self.assertEqual((output / "data/tilesets/cormoria/primary/general/tiles.png").read_bytes(),
                             (self.foundation / "source/data/tilesets/primary/general/tiles.png").read_bytes())
            self.assertTrue((output / "data/tilesets/cormoria/primary/general/palettes/15.pal").is_file())
            self.assertFalse((output / "data/tilesets/primary/general/metatiles.bin").exists())
            self.assertEqual((output / "data/tilesets/cormoria/primary/general/metatiles.bin").read_bytes(),
                             (self.stage / "source/data/tilesets/primary/general/metatiles.bin").read_bytes())
            with self.assertRaises(materialize_tileset_preview.MaterializationError):
                materialize_tileset_preview.materialize(
                    self.stage, self.foundation, self.rules, self.anims, preview, output, source_only=True)

    def test_modified_preview_cannot_drive_materialization(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            preview = root / "preview"
            preview.mkdir()
            for name, content in register_tilesets.render(self.stage, self.foundation, self.rules, self.anims).items():
                (preview / name).write_bytes(content)
            (preview / "tilesets.c").write_bytes(b"changed")
            with self.assertRaisesRegex(materialize_tileset_preview.MaterializationError,
                                        "differs from authenticated stage"):
                materialize_tileset_preview.materialize(
                    self.stage, self.foundation, self.rules, self.anims, preview, root / "assets", source_only=True)

    def test_donor_output_comparison_detects_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "output"
            donor = root / "donor"
            (output / "data/tilesets/cormoria/primary/general").mkdir(parents=True)
            (donor / "data/tilesets/primary/general").mkdir(parents=True)
            target = output / "data/tilesets/cormoria/primary/general/metatiles.bin"
            expected = donor / "data/tilesets/primary/general/metatiles.bin"
            target.write_bytes(b"match")
            expected.write_bytes(b"match")
            plan = {"recipes": [{"requested": "data/tilesets/primary/general/metatiles.bin",
                                 "target": "data/tilesets/cormoria/primary/general/metatiles.bin"}]}
            self.assertEqual(materialize_tileset_preview.verify_donor_outputs(output, plan, donor), 1)
            target.write_bytes(b"drift")
            with self.assertRaisesRegex(materialize_tileset_preview.MaterializationError,
                                        "1 donor output mismatches"):
                materialize_tileset_preview.verify_donor_outputs(output, plan, donor)

    @unittest.skipUnless(os.environ.get("CORMORIA_MATERIALIZED") and os.environ.get("CORMORIA_DONOR_ARCHIVE"),
                         "native donor build comparison unavailable")
    def test_all_materialized_outputs_match_donor_build(self) -> None:
        output = Path(os.environ["CORMORIA_MATERIALIZED"])
        plan = json.loads((output / "asset_plan.json").read_text(encoding="utf-8"))
        self.assertEqual(materialize_tileset_preview.verify_donor_outputs(
            output, plan, Path(os.environ["CORMORIA_DONOR_ARCHIVE"])), 1058)


if __name__ == "__main__":
    unittest.main()
